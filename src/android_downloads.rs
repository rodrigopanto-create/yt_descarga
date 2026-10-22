use jni::objects::{JObject, JValue};
use jni::JNIEnv;
use std::fs::{self, File};
use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DOWNLOAD: AtomicU64 = AtomicU64::new(0);

pub(crate) fn prepare_download() -> Result<PathBuf, String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let sdk = env
        .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")
        .and_then(|v| v.i())
        .map_err(|e| format!("No se pudo consultar la versión Android: {e}"))?;
    if sdk < 29 {
        return Err(
            "Guardar en Descargas mediante MediaStore requiere Android 10 o posterior.".into(),
        );
    }
    let parent = crate::android_yt_dlp::files_dir()?.join("download-staging");
    fs::create_dir_all(&parent).map_err(|e| format!("No se pudo crear staging: {e}"))?;
    loop {
        let path = parent.join(format!(
            "{}-{}",
            std::process::id(),
            NEXT_DOWNLOAD.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("No se pudo crear staging de descarga: {e}")),
        }
    }
}

pub(crate) fn publish_download(directory: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory).map_err(|e| format!("No se pudo leer staging: {e}"))? {
        let entry = entry.map_err(|e| format!("No se pudo leer un archivo descargado: {e}"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|e| format!("No se pudo inspeccionar la descarga: {e}"))?;
        let path = entry.path();
        if !metadata.is_file()
            || metadata.len() == 0
            || matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("part" | "ytdl" | "tmp")
            )
        {
            return Err(format!(
                "Salida incompleta o no regular: {}",
                path.display()
            ));
        }
        files.push(path);
    }
    if files.is_empty() {
        return Err("yt-dlp terminó sin generar archivos; no se publicó ninguna descarga.".into());
    }
    files.sort();
    let mut published = Vec::new();
    for path in files {
        let uri = publish_file(&path).map_err(|e| {
            format!(
                "{e}. Publicados previamente: {published:?}. Se conserva staging en {}",
                directory.display()
            )
        })?;
        published.push(uri);
        if let Err(e) = fs::remove_file(&path) {
            log::warn!(
                "Descarga publicada; no se pudo limpiar {}: {e}",
                path.display()
            );
        }
    }
    if let Err(e) = fs::remove_dir(directory) {
        log::warn!("No se pudo limpiar {}: {e}", directory.display());
    }
    Ok(published)
}

fn put_string(
    env: &mut JNIEnv<'_>,
    values: &JObject<'_>,
    key: &str,
    value: &str,
) -> jni::errors::Result<()> {
    let key = env.new_string(key)?;
    let value = env.new_string(value)?;
    env.call_method(
        values,
        "put",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[JValue::Object(key.as_ref()), JValue::Object(value.as_ref())],
    )?;
    Ok(())
}

fn put_pending(
    env: &mut JNIEnv<'_>,
    values: &JObject<'_>,
    pending: i32,
) -> jni::errors::Result<()> {
    let key = env.new_string("is_pending")?;
    let value = env
        .call_static_method(
            "java/lang/Integer",
            "valueOf",
            "(I)Ljava/lang/Integer;",
            &[JValue::Int(pending)],
        )?
        .l()?;
    env.call_method(
        values,
        "put",
        "(Ljava/lang/String;Ljava/lang/Integer;)V",
        &[JValue::Object(key.as_ref()), JValue::Object(&value)],
    )?;
    Ok(())
}

fn publish_file(path: &Path) -> Result<String, String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let result = env.with_local_frame(32, |env| -> jni::errors::Result<Result<String, String>> {
        Ok((|| -> Result<String, String> {
        let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
        let resolver = env.call_method(&activity, "getContentResolver", "()Landroid/content/ContentResolver;", &[])
            .and_then(|v| v.l()).map_err(|e| e.to_string())?;
        let collection = env.get_static_field("android/provider/MediaStore$Downloads", "EXTERNAL_CONTENT_URI", "Landroid/net/Uri;")
            .and_then(|v| v.l()).map_err(|e| e.to_string())?;
        let values = env.new_object("android/content/ContentValues", "()V", &[]).map_err(|e| e.to_string())?;
        let name = path.file_name().and_then(|v| v.to_str()).ok_or("Nombre de descarga no válido")?;
        let mime = match path.extension().and_then(|v| v.to_str()) {
            Some("mp4" | "m4v") => "video/mp4",
            Some("webm") => "video/webm",
            Some("mkv") => "video/x-matroska",
            Some("mp3") => "audio/mpeg",
            Some("m4a") => "audio/mp4",
            Some("ogg" | "opus") => "audio/ogg",
            _ => "application/octet-stream",
        };
        put_string(env, &values, "_display_name", name).map_err(|e| e.to_string())?;
        put_string(env, &values, "mime_type", mime).map_err(|e| e.to_string())?;
        put_string(env, &values, "relative_path", "Download/yt_descarga").map_err(|e| e.to_string())?;
        put_pending(env, &values, 1).map_err(|e| e.to_string())?;
        let uri = env.call_method(&resolver, "insert", "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;",
            &[JValue::Object(&collection), JValue::Object(&values)])
            .and_then(|v| v.l()).map_err(|e| format!("MediaStore.insert falló: {e}"))?;
        if uri.is_null() {
            return Err("MediaStore no creó una entrada para la descarga".into());
        }
        let copy_result = (|| -> Result<String, String> {
            let mode = env.new_string("w").map_err(|e| e.to_string())?;
            let descriptor = env.call_method(&resolver, "openFileDescriptor",
                "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;",
                &[JValue::Object(&uri), JValue::Object(mode.as_ref())])
                .and_then(|v| v.l()).map_err(|e| format!("No se pudo abrir destino MediaStore: {e}"))?;
            if descriptor.is_null() {
                return Err("MediaStore devolvió un descriptor nulo".into());
            }
            let fd = env.call_method(&descriptor, "detachFd", "()I", &[])
                .and_then(|v| v.i()).map_err(|e| e.to_string())?;
            if fd < 0 {
                return Err("MediaStore devolvió un descriptor inválido".into());
            }
            // detachFd transfers ownership to Rust; close explicitly before publishing.
            let mut destination = unsafe { File::from_raw_fd(fd) };
            let mut source = File::open(path).map_err(|e| format!("No se pudo abrir descarga: {e}"))?;
            let expected = source.metadata().map_err(|e| e.to_string())?.len();
            let copied = std::io::copy(&mut source, &mut destination).map_err(|e| format!("Error copiando a MediaStore: {e}"))?;
            if copied != expected {
                return Err(format!("Copia incompleta: {copied}/{expected} bytes"));
            }
            destination.sync_all().map_err(|e| format!("Error sincronizando MediaStore: {e}"))?;
            use std::os::fd::IntoRawFd;
            if unsafe { libc::close(destination.into_raw_fd()) } != 0 {
                return Err(format!("Error cerrando MediaStore: {}", std::io::Error::last_os_error()));
            }
            let uri_text = env.call_method(&uri, "toString", "()Ljava/lang/String;", &[])
                .and_then(|v| v.l()).map_err(|e| e.to_string())?;
            let uri_text: String = env.get_string((&uri_text).into()).map_err(|e| e.to_string())?.into();
            env.call_method(&values, "clear", "()V", &[]).map_err(|e| e.to_string())?;
            put_pending(env, &values, 0).map_err(|e| e.to_string())?;
            let updated = env.call_method(&resolver, "update",
                "(Landroid/net/Uri;Landroid/content/ContentValues;Ljava/lang/String;[Ljava/lang/String;)I",
                &[JValue::Object(&uri), JValue::Object(&values), JValue::Object(&JObject::null()), JValue::Object(&JObject::null())])
                .and_then(|v| v.i()).map_err(|e| format!("No se pudo publicar descarga: {e}"))?;
            if updated != 1 {
                return Err(format!("MediaStore actualizó {updated} entradas en vez de una"));
            }
            Ok(uri_text)
        })();
        if copy_result.is_err() {
            clear_exception(env);
            match env.call_method(&resolver, "delete", "(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I",
                &[JValue::Object(&uri), JValue::Object(&JObject::null()), JValue::Object(&JObject::null())]).and_then(|v| v.i()) {
                Ok(1) => {}
                Ok(count) => log::error!("No se eliminó la entrada MediaStore incompleta: {count}"),
                Err(e) => log::error!("No se pudo eliminar la entrada MediaStore incompleta: {e}"),
            }
        }
        copy_result
        })())
    }).map_err(|e| format!("No se pudo crear el contexto JNI de MediaStore: {e}"))
        .and_then(|result| result);
    if result.is_err() {
        clear_exception(&mut env);
    }
    result
}

fn clear_exception(env: &mut JNIEnv<'_>) {
    match env.exception_check() {
        Ok(true) => {
            if let Err(e) = env.exception_describe() {
                log::error!("No se pudo describir excepción JNI: {e}");
            }
            if let Err(e) = env.exception_clear() {
                log::error!("No se pudo limpiar excepción JNI: {e}");
            }
        }
        Ok(false) => {}
        Err(e) => log::error!("No se pudo consultar excepción JNI: {e}"),
    }
}
