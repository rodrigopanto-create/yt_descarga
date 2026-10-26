#[cfg(target_os = "android")]
pub(crate) fn publicar_descarga(path: &std::path::Path) -> Result<String, String> {
    use jni::objects::{JObject, JValue};
    use std::fs;
    use std::io::{self, Write};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::path::Path;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("Nombre de archivo no válido: {}", path.display()))?;
    let mime_type = mime_type_for(path);
    let ctx = ndk_context::android_context();
    if ctx.context().is_null() {
        return Err("El contexto Android no está disponible para guardar la descarga".to_string());
    }

    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
        .map_err(|error| format!("No se pudo acceder a la JVM: {error}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|error| format!("No se pudo adjuntar a la JVM: {error}"))?;
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let sdk_version = env
        .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")
        .and_then(|value| value.i())
        .map_err(|error| format!("No se pudo consultar la versión de Android: {error}"))?;

    if sdk_version < 29 {
        let downloads_name = env
            .new_string("Download")
            .map_err(|error| format!("No se pudo crear el nombre de carpeta: {error}"))?;
        let directory = env
            .call_method(
                &activity,
                "getExternalFilesDir",
                "(Ljava/lang/String;)Ljava/io/File;",
                &[JValue::Object(&downloads_name)],
            )
            .and_then(|value| value.l())
            .map_err(|error| {
                format!("No se pudo obtener el directorio de descargas de la app: {error}")
            })?;
        if directory.is_null() {
            return Err("Android no proporcionó almacenamiento externo para la app".to_string());
        }
        let directory_path = env
            .call_method(&directory, "getAbsolutePath", "()Ljava/lang/String;", &[])
            .and_then(|value| value.l())
            .map_err(|error| format!("No se pudo resolver el directorio de descargas: {error}"))?;
        let directory_path: String = env
            .get_string((&directory_path).into())
            .map_err(|error| format!("No se pudo leer el directorio de descargas: {error}"))?
            .into();
        let directory_path = Path::new(&directory_path);
        fs::create_dir_all(directory_path)
            .map_err(|error| format!("No se pudo crear {}: {error}", directory_path.display()))?;
        let destination = unique_destination(directory_path, file_name);
        fs::copy(path, &destination).map_err(|error| {
            format!(
                "No se pudo copiar la descarga a {}: {error}",
                destination.display()
            )
        })?;
        return Ok(format!(
            "{} (carpeta privada de la app; Android anterior a 10)",
            destination.display()
        ));
    }

    let resolver = env
        .call_method(
            &activity,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )
        .and_then(|value| value.l())
        .map_err(|error| format!("No se pudo acceder al almacenamiento multimedia: {error}"))?;
    let uri_string = env
        .new_string("content://media/external/downloads")
        .map_err(|error| format!("No se pudo crear la URI de Descargas: {error}"))?;
    let collection_uri = env
        .call_static_method(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&uri_string)],
        )
        .and_then(|value| value.l())
        .map_err(|error| format!("No se pudo resolver la colección de Descargas: {error}"))?;
    let values = env
        .new_object("android/content/ContentValues", "()V", &[])
        .map_err(|error| format!("No se pudieron preparar los datos de la descarga: {error}"))?;
    put_string(&mut env, &values, "_display_name", file_name)?;
    put_string(&mut env, &values, "mime_type", mime_type)?;
    put_string(&mut env, &values, "relative_path", "Download/yt_descarga")?;
    let pending_key = env
        .new_string("is_pending")
        .map_err(|error| format!("No se pudo crear el campo de publicación: {error}"))?;
    let pending_value = env
        .call_static_method(
            "java/lang/Integer",
            "valueOf",
            "(I)Ljava/lang/Integer;",
            &[1.into()],
        )
        .and_then(|value| value.l())
        .map_err(|error| format!("No se pudo marcar la descarga como temporal: {error}"))?;
    env.call_method(
        &values,
        "put",
        "(Ljava/lang/String;Ljava/lang/Integer;)V",
        &[JValue::Object(&pending_key), JValue::Object(&pending_value)],
    )
    .map_err(|error| format!("No se pudo marcar la descarga como temporal: {error}"))?;

    let uri = env
        .call_method(
            &resolver,
            "insert",
            "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;",
            &[JValue::Object(&collection_uri), JValue::Object(&values)],
        )
        .and_then(|value| value.l())
        .map_err(|error| {
            match env.exception_check() {
                Ok(true) => {
                    if let Err(clear_error) = env.exception_clear() {
                        return format!(
                            "No se pudo crear el archivo en Descargas: {error}; no se pudo limpiar la excepción JNI: {clear_error}"
                        );
                    }
                }
                Ok(false) => {}
                Err(check_error) => {
                    return format!(
                        "No se pudo crear el archivo en Descargas: {error}; no se pudo consultar la excepción JNI: {check_error}"
                    );
                }
            }
            format!("No se pudo crear el archivo en Descargas: {error}")
        })?;
    if uri.is_null() {
        return Err("Android no pudo crear el archivo en Descargas".to_string());
    }

    let publish_result = (|| {
        let write_mode = env
            .new_string("w")
            .map_err(|error| format!("No se pudo abrir el archivo de Descargas: {error}"))?;
        let descriptor = env
            .call_method(
                &resolver,
                "openFileDescriptor",
                "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;",
                &[JValue::Object(&uri), JValue::Object(&write_mode)],
            )
            .and_then(|value| value.l())
            .map_err(|error| format!("No se pudo abrir el archivo de Descargas: {error}"))?;
        if descriptor.is_null() {
            return Err("Android no proporcionó un descriptor para guardar el archivo".to_string());
        }
        let fd = env
            .call_method(&descriptor, "detachFd", "()I", &[])
            .and_then(|value| value.i())
            .map_err(|error| format!("No se pudo obtener el descriptor de salida: {error}"))?;
        if fd < 0 {
            return Err("Android devolvió un descriptor de archivo no válido".to_string());
        }
        let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let mut output = fs::File::from(owned_fd);
        let mut input = fs::File::open(path)
            .map_err(|error| format!("No se pudo leer {}: {error}", path.display()))?;
        io::copy(&mut input, &mut output)
            .map_err(|error| format!("No se pudo copiar el archivo a Descargas: {error}"))?;
        output
            .flush()
            .map_err(|error| format!("No se pudo completar la escritura en Descargas: {error}"))?;
        drop(output);

        let published_values = env
            .new_object("android/content/ContentValues", "()V", &[])
            .map_err(|error| format!("No se pudo finalizar la publicación: {error}"))?;
        let pending_key = env
            .new_string("is_pending")
            .map_err(|error| format!("No se pudo finalizar la publicación: {error}"))?;
        let pending_value = env
            .call_static_method(
                "java/lang/Integer",
                "valueOf",
                "(I)Ljava/lang/Integer;",
                &[0.into()],
            )
            .and_then(|value| value.l())
            .map_err(|error| format!("No se pudo finalizar la publicación: {error}"))?;
        env.call_method(
            &published_values,
            "put",
            "(Ljava/lang/String;Ljava/lang/Integer;)V",
            &[JValue::Object(&pending_key), JValue::Object(&pending_value)],
        )
        .map_err(|error| format!("No se pudo finalizar la publicación: {error}"))?;
        let null = JObject::null();
        let updated = if sdk_version >= 30 {
        env.call_method(
            &resolver,
            "update",
            "(Landroid/net/Uri;Landroid/content/ContentValues;Landroid/os/Bundle;)I",
            &[
                JValue::Object(&uri),
                JValue::Object(&published_values),
                JValue::Object(&null),
            ],
        )
        } else {
        env.call_method(
            &resolver,
            "update",
            "(Landroid/net/Uri;Landroid/content/ContentValues;[Ljava/lang/String;[Ljava/lang/String;)I",
            &[
                JValue::Object(&uri),
                JValue::Object(&published_values),
                JValue::Object(&null),
                JValue::Object(&null),
            ],
        )
        }
        .and_then(|value| value.i())
        .map_err(|error| format!("No se pudo hacer visible el archivo en Descargas: {error}"))?;
        if updated == 0 {
            return Err("Android no encontró el archivo de Descargas al publicarlo".to_string());
        }

        Ok(format!("Descargas/yt_descarga/{file_name}"))
    })();

    if publish_result.is_err() {
        let null = JObject::null();
        match env.exception_check() {
            Ok(true) => {
                if let Err(error) = env.exception_clear() {
                    log::error!("No se pudo limpiar la excepción JNI antes de borrar el archivo incompleto: {error}");
                    return publish_result;
                }
            }
            Ok(false) => {}
            Err(error) => {
                log::error!("No se pudo consultar la excepción JNI al limpiar el archivo incompleto: {error}");
                return publish_result;
            }
        }
        let delete_result = if sdk_version >= 30 {
            env.call_method(
                &resolver,
                "delete",
                "(Landroid/net/Uri;Landroid/os/Bundle;)I",
                &[JValue::Object(&uri), JValue::Object(&null)],
            )
        } else {
            env.call_method(
                &resolver,
                "delete",
                "(Landroid/net/Uri;[Ljava/lang/String;[Ljava/lang/String;)I",
                &[
                    JValue::Object(&uri),
                    JValue::Object(&null),
                    JValue::Object(&null),
                ],
            )
        };
        if let Err(error) = delete_result {
            log::error!("No se pudo limpiar el archivo incompleto de Descargas: {error}");
            match env.exception_check() {
                Ok(true) => {
                    if let Err(clear_error) = env.exception_clear() {
                        log::error!("No se pudo limpiar la excepción JNI después del borrado fallido: {clear_error}");
                    }
                }
                Ok(false) => {}
                Err(check_error) => {
                    log::error!("No se pudo consultar la excepción JNI después del borrado fallido: {check_error}");
                }
            }
        }
    }

    publish_result
}

#[cfg(target_os = "android")]
fn put_string(
    env: &mut jni::JNIEnv<'_>,
    values: &jni::objects::JObject<'_>,
    key: &str,
    value: &str,
) -> Result<(), String> {
    let key = env
        .new_string(key)
        .map_err(|error| format!("No se pudo preparar el nombre del archivo: {error}"))?;
    let value = env
        .new_string(value)
        .map_err(|error| format!("No se pudo preparar el nombre del archivo: {error}"))?;
    env.call_method(
        values,
        "put",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[
            jni::objects::JValue::Object(&key),
            jni::objects::JValue::Object(&value),
        ],
    )
    .map_err(|error| format!("No se pudieron preparar los metadatos del archivo: {error}"))?;
    Ok(())
}

#[cfg(target_os = "android")]
fn unique_destination(directory: &std::path::Path, file_name: &str) -> std::path::PathBuf {
    use std::path::Path;

    let original = directory.join(file_name);
    if !original.exists() {
        return original;
    }
    let path = Path::new(file_name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path.extension().map(|value| value.to_string_lossy());
    for suffix in 1.. {
        let name = match &extension {
            Some(extension) => format!("{stem} ({suffix}).{extension}"),
            None => format!("{stem} ({suffix})"),
        };
        let candidate = directory.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("El contador de nombres siempre termina encontrando un candidato");
}

#[cfg(target_os = "android")]
fn mime_type_for(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "opus" => "audio/opus",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

#[cfg(target_os = "android")]
pub fn obtener_portapapeles_jni() -> Option<String> {
    use jni::objects::JObject;

    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;

    let raw_context = ctx.context() as *mut jni::sys::_jobject;
    let activity = unsafe { JObject::from_raw(raw_context) };

    let clipboard_service_str = env.new_string("clipboard").ok()?;
    let clipboard_mgr = env
        .call_method(
            &activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[(&clipboard_service_str).into()],
        )
        .ok()?
        .l()
        .ok()?;

    if clipboard_mgr.is_null() {
        return None;
    }

    let primary_clip = env
        .call_method(
            &clipboard_mgr,
            "getPrimaryClip",
            "()Landroid/content/ClipData;",
            &[],
        )
        .ok()?
        .l()
        .ok()?;

    if primary_clip.is_null() {
        return None;
    }

    let item = env
        .call_method(
            &primary_clip,
            "getItemAt",
            "(I)Landroid/content/ClipData$Item;",
            &[0.into()],
        )
        .ok()?
        .l()
        .ok()?;

    if item.is_null() {
        return None;
    }

    let char_seq = env
        .call_method(
            &item,
            "coerceToText",
            "(Landroid/content/Context;)Ljava/lang/CharSequence;",
            &[(&activity).into()],
        )
        .ok()?
        .l()
        .ok()?;

    if char_seq.is_null() {
        return None;
    }

    let string_obj = env
        .call_method(&char_seq, "toString", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    let rust_str: String = env.get_string((&string_obj).into()).ok()?.into();
    Some(rust_str)
}
