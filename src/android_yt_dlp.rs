//! Runtime Android basado en CPython compilado para Bionic y el zipapp oficial de yt-dlp.
//!
//! Artefactos requeridos en el APK:
//! - `jniLibs/arm64-v8a/libpython3.so`: ejecutable PIE AArch64 enlazado para Bionic.
//! - Asset `android-runtime/manifest.json`: versiones, hashes y lista de archivos de Python.
//! - Los archivos listados por el manifiesto y `android-runtime/yt-dlp.pyz`.
//!
//! `libytdlp.so` no se trata como un CLI: los nombres de biblioteca no demuestran que un ELF
//! tenga un entrypoint ni que use el cargador de Android. El manifiesto y los artefactos deben
//! estar dentro del APK firmado; sus hashes detectan sustituciones/corrupción del empaquetado.

use std::path::PathBuf;
use std::sync::mpsc;

#[cfg(any(target_os = "android", test))]
use serde::Deserialize;
#[cfg(any(target_os = "android", test))]
use std::path::Path;

#[cfg(target_os = "android")]
const MANIFEST_ASSET: &str = "android-runtime/manifest.json";
#[cfg(target_os = "android")]
const YTDLP_ASSET: &str = "android-runtime/yt-dlp.pyz";
#[cfg(target_os = "android")]
const PYTHON_EXECUTABLE: &str = "libpython3.so";
#[cfg(target_os = "android")]
const YTDLP_WRAPPER: &str = "yt-dlp";
#[cfg(any(target_os = "android", test))]
const MAX_MANIFEST_BYTES: usize = 2 * 1024 * 1024;
#[cfg(any(target_os = "android", test))]
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
#[cfg(target_os = "android")]
const MAX_RUNTIME_BYTES: u64 = 1024 * 1024 * 1024;
#[cfg(any(target_os = "android", test))]
const MAX_RUNTIME_FILES: usize = 4096;
#[cfg(target_os = "android")]
const PROCESS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

#[cfg(target_os = "android")]
use std::ffi::CString;
#[cfg(target_os = "android")]
use std::fs::{self, File, OpenOptions};
#[cfg(target_os = "android")]
use std::io::{self, Read, Seek, Write};
#[cfg(target_os = "android")]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_os = "android")]
use std::process::{Command, Stdio};
#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "android")]
use std::sync::Mutex;

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

#[cfg(target_os = "android")]
static ANDROID_APP: std::sync::OnceLock<AndroidApp> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
static PREPARE_LOCK: Mutex<()> = Mutex::new(());

#[cfg(any(target_os = "android", test))]
#[derive(Clone, Debug, Deserialize)]
struct RuntimeManifest {
    schema_version: u32,
    runtime_version: String,
    python_version: String,
    python_executable_sha256: String,
    ytdlp_version: String,
    ytdlp_sha256: String,
    python_files: Vec<RuntimeFile>,
    #[serde(default)]
    ffmpeg_sha256: Option<String>,
    #[serde(default)]
    ffprobe_sha256: Option<String>,
    #[serde(default)]
    ffmpeg_version: Option<String>,
}

#[cfg(any(target_os = "android", test))]
#[derive(Clone, Debug, Deserialize)]
struct RuntimeFile {
    asset: String,
    install_path: String,
    sha256: String,
}

#[cfg(target_os = "android")]
#[derive(Debug)]
pub(crate) struct AndroidTools {
    pub ytdlp: PathBuf,
    pub ffmpeg: Option<PathBuf>,
}

#[cfg(target_os = "android")]
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "android")]
pub fn initialize_android_assets(app: AndroidApp) -> Result<(), String> {
    ANDROID_APP
        .set(app)
        .map_err(|_| "El contexto Android ya fue inicializado".to_string())
}

#[cfg(target_os = "android")]
pub fn app_files_dir() -> PathBuf {
    files_dir().unwrap_or_else(|error| {
        log::error!("No se pudo obtener Context.getFilesDir(): {error}");
        PathBuf::from("/data/data/com.rodrigo.yt_descarga/files")
    })
}

#[cfg(target_os = "android")]
pub(crate) fn files_dir() -> Result<PathBuf, String> {
    let ctx = ndk_context::android_context();
    if ctx.context().is_null() {
        return Err("El contexto Android no está disponible".to_string());
    }
    unsafe {
        let vm = jni::JavaVM::from_raw(ctx.vm().cast())
            .map_err(|e| format!("No se pudo acceder a la JVM: {e}"))?;
        let mut env = vm
            .attach_current_thread()
            .map_err(|e| format!("No se pudo adjuntar a la JVM: {e}"))?;
        let activity = jni::objects::JObject::from_raw(ctx.context().cast());
        let files = env
            .call_method(&activity, "getFilesDir", "()Ljava/io/File;", &[])
            .and_then(|v| v.l())
            .map_err(|e| format!("Context.getFilesDir() falló: {e}"))?;
        let path = env
            .call_method(&files, "getAbsolutePath", "()Ljava/lang/String;", &[])
            .and_then(|v| v.l())
            .map_err(|e| format!("File.getAbsolutePath() falló: {e}"))?;
        let path: String = env
            .get_string((&path).into())
            .map_err(|e| format!("No se pudo leer la ruta de la app: {e}"))?
            .into();
        Ok(PathBuf::from(path))
    }
}

#[cfg(target_os = "android")]
fn native_lib_dir() -> Result<PathBuf, String> {
    let ctx = ndk_context::android_context();
    if ctx.context().is_null() {
        return Err("El contexto Android no está disponible".to_string());
    }
    unsafe {
        let vm = jni::JavaVM::from_raw(ctx.vm().cast())
            .map_err(|e| format!("No se pudo acceder a la JVM: {e}"))?;
        let mut env = vm
            .attach_current_thread()
            .map_err(|e| format!("No se pudo adjuntar a la JVM: {e}"))?;
        let activity = jni::objects::JObject::from_raw(ctx.context().cast());
        let info = env
            .call_method(
                &activity,
                "getApplicationInfo",
                "()Landroid/content/pm/ApplicationInfo;",
                &[],
            )
            .and_then(|v| v.l())
            .map_err(|e| format!("No se pudo obtener ApplicationInfo: {e}"))?;
        let path = env
            .get_field(&info, "nativeLibraryDir", "Ljava/lang/String;")
            .and_then(|v| v.l())
            .map_err(|e| format!("No se pudo obtener nativeLibraryDir: {e}"))?;
        let path: String = env
            .get_string((&path).into())
            .map_err(|e| format!("No se pudo leer nativeLibraryDir: {e}"))?
            .into();
        Ok(PathBuf::from(path))
    }
}

/// Alto (en px físicos) del área ocupada por la barra de estado/notificaciones.
#[cfg(target_os = "android")]
pub fn content_rect_top_px() -> i32 {
    ANDROID_APP
        .get()
        .map(|app| app.content_rect().top)
        .unwrap_or(0)
}

/// Comprueba artefactos, prepara el runtime versionado y crea el wrapper `sh` de yt-dlp.
#[cfg(target_os = "android")]
pub(crate) fn prepare_android_tools() -> Result<AndroidTools, String> {
    let _guard = PREPARE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let app_files = files_dir()?;
    let native_dir = native_lib_dir()?;
    let runtime_root = app_files.join("python-runtime");
    let manifest = read_manifest()?;
    validate_manifest(&manifest)?;

    let python = native_dir.join(PYTHON_EXECUTABLE);
    validate_python_executable(&python, &manifest.python_executable_sha256)?;

    let runtime_dir = runtime_root.join(&manifest.runtime_version);
    match fs::symlink_metadata(&runtime_dir) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            validate_installed_runtime(&runtime_dir, &manifest)?;
            verify_python(
                &python,
                &runtime_dir.join("python"),
                &native_dir,
                &manifest.python_version,
            )?;
            verify_ytdlp(
                &python,
                &runtime_dir.join("python"),
                &native_dir,
                &runtime_dir.join("yt-dlp.pyz"),
                &manifest.ytdlp_version,
            )?;
        }
        Ok(_) => {
            return Err(format!(
                "La ruta de runtime versionado existe pero no es un directorio regular: {}",
                runtime_dir.display()
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            install_runtime(
                &app_files.join("tmp"),
                &runtime_root,
                &runtime_dir,
                &manifest,
                &python,
                &native_dir,
            )?;
        }
        Err(error) => {
            return Err(format!(
                "No se pudo inspeccionar el runtime {}: {error}",
                runtime_dir.display()
            ));
        }
    }

    let python_home = runtime_dir.join("python");
    let ytdlp_zipapp = runtime_dir.join("yt-dlp.pyz");
    let wrapper_dir = app_files.join("bin");
    fs::create_dir_all(&wrapper_dir)
        .map_err(|e| format!("No se pudo crear {}: {e}", wrapper_dir.display()))?;
    let ca_directory = match fs::metadata("/apex/com.android.conscrypt/cacerts") {
        Ok(metadata) if metadata.is_dir() => Path::new("/apex/com.android.conscrypt/cacerts"),
        Ok(_) => return Err("El almacén de CA Android no es un directorio".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Path::new("/system/etc/security/cacerts")
        }
        Err(error) => {
            return Err(format!(
                "No se pudo consultar el almacén CA Android: {error}"
            ))
        }
    };
    let ca_bundle = wrapper_dir.join("android-ca.pem");
    write_ca_bundle(ca_directory, &ca_bundle)?;
    let wrapper = wrapper_dir.join(YTDLP_WRAPPER);
    install_wrapper(&wrapper, &python, &python_home, &ytdlp_zipapp, &ca_bundle)?;

    let ffmpeg = manifest.ffmpeg_sha256.as_deref().and_then(|hash| {
        match validate_ffmpeg_tools(&native_dir, hash, &manifest) {
            Ok(path) => Some(path),
            Err(error) => {
                log::warn!("ffmpeg Android no disponible; yt-dlp seguirá funcionando: {error}");
                None
            }
        }
    });

    Ok(AndroidTools {
        ytdlp: wrapper,
        ffmpeg,
    })
}

/// API de compatibilidad: devuelve el wrapper que `YtDlpCommand` ejecuta mediante `/system/bin/sh`.
#[cfg(target_os = "android")]
pub fn ensure_yt_dlp_executable() -> Result<PathBuf, String> {
    prepare_android_tools().map(|tools| tools.ytdlp)
}

#[cfg(target_os = "android")]
fn read_manifest() -> Result<RuntimeManifest, String> {
    let app = ANDROID_APP
        .get()
        .ok_or_else(|| "AssetManager no fue inicializado desde android_main".to_string())?;
    let name = CString::new(MANIFEST_ASSET).map_err(|e| e.to_string())?;
    let mut asset = app.asset_manager().open(&name).ok_or_else(|| {
        format!(
            "Falta el runtime Android empaquetado: asset {MANIFEST_ASSET}. \
                 Consulta assets/README.md para CPython NDK + yt-dlp zipapp."
        )
    })?;
    if asset.length() > MAX_MANIFEST_BYTES {
        return Err(format!("El manifiesto supera {MAX_MANIFEST_BYTES} bytes"));
    }
    let mut bytes = Vec::with_capacity(asset.length());
    asset
        .read_to_end(&mut bytes)
        .map_err(|e| format!("No se pudo leer {MANIFEST_ASSET}: {e}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("Manifiesto Android inválido ({MANIFEST_ASSET}): {e}"))
}

#[cfg(any(target_os = "android", test))]
fn validate_manifest(m: &RuntimeManifest) -> Result<(), String> {
    if m.schema_version != 1 {
        return Err(format!(
            "Versión de esquema Android no soportada: {}",
            m.schema_version
        ));
    }
    if !safe_component(&m.runtime_version) {
        return Err("runtime_version debe ser un componente de ruta seguro".to_string());
    }
    if !is_version(&m.python_version, 3) {
        return Err(format!(
            "Versión CPython inválida: {} (se esperaba 3.x.y)",
            m.python_version
        ));
    }
    if !is_version(&m.ytdlp_version, 3) {
        return Err(format!("Versión yt-dlp inválida: {}", m.ytdlp_version));
    }
    validate_hash(&m.python_executable_sha256, "python_executable_sha256")?;
    validate_hash(&m.ytdlp_sha256, "ytdlp_sha256")?;
    if let Some(hash) = &m.ffmpeg_sha256 {
        validate_hash(hash, "ffmpeg_sha256")?;
    }
    match (&m.ffmpeg_sha256, &m.ffprobe_sha256, &m.ffmpeg_version) {
        (None, None, None) => {}
        (Some(_), Some(hash), Some(version)) if is_version(version, 3) => {
            validate_hash(hash, "ffprobe_sha256")?;
        }
        _ => return Err("FFmpeg requiere hashes de ffmpeg y ffprobe y una versión x.y.z".into()),
    }
    if m.python_files.is_empty() || m.python_files.len() > MAX_RUNTIME_FILES {
        return Err(format!(
            "El manifiesto debe listar entre 1 y {MAX_RUNTIME_FILES} archivos de Python"
        ));
    }

    let mut rutas = std::collections::HashSet::new();
    for file in &m.python_files {
        let asset = Path::new(&file.asset);
        if !is_safe_relative_path(asset) || !file.asset.starts_with("android-runtime/python/") {
            return Err(format!("Ruta de asset de Python inválida: {}", file.asset));
        }
        let install_path = Path::new(&file.install_path);
        if !is_safe_relative_path(install_path) {
            return Err(format!(
                "Ruta de instalación de Python inválida: {}",
                file.install_path
            ));
        }
        if !rutas.insert(file.install_path.clone()) {
            return Err(format!(
                "Ruta de instalación duplicada: {}",
                file.install_path
            ));
        }
        validate_hash(&file.sha256, &file.install_path)?;
    }

    let minor = m.python_version.split('.').nth(1).unwrap_or_default();
    let stdlib_zip = format!("lib/python3.{minor}/python3{minor}.zip");
    let expanded = [
        format!("lib/python3.{minor}/os.py"),
        format!("lib/python3.{minor}/encodings/__init__.py"),
    ];
    if !rutas.contains(&stdlib_zip) && !expanded.iter().all(|path| rutas.contains(path)) {
        return Err(format!(
            "Falta la biblioteca estándar de CPython: {stdlib_zip} o los módulos expandidos os.py y encodings/__init__.py"
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "android", test))]
fn is_safe_relative_path(path: &std::path::Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

#[cfg(any(target_os = "android", test))]
fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

#[cfg(any(target_os = "android", test))]
fn validate_hash(hash: &str, field: &str) -> Result<(), String> {
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "{field} debe ser un SHA-256 hexadecimal de 64 caracteres"
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "android", test))]
fn is_version(value: &str, parts: usize) -> bool {
    let values: Vec<_> = value.split('.').collect();
    values.len() == parts
        && values
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(target_os = "android")]
fn install_runtime(
    temp_root: &Path,
    runtime_root: &Path,
    runtime_dir: &Path,
    manifest: &RuntimeManifest,
    python: &Path,
    native_dir: &Path,
) -> Result<(), String> {
    fs::create_dir_all(runtime_root)
        .map_err(|e| format!("No se pudo crear {}: {e}", runtime_root.display()))?;
    fs::create_dir_all(temp_root)
        .map_err(|e| format!("No se pudo crear {}: {e}", temp_root.display()))?;
    cleanup_staging_dirs(temp_root);
    let staging = create_staging_dir(temp_root)?;
    let mut guard = StagingDir {
        path: staging.clone(),
        committed: false,
    };
    let python_home = staging.join("python");
    fs::create_dir_all(&python_home)
        .map_err(|e| format!("No se pudo crear {}: {e}", python_home.display()))?;

    let mut total = 0u64;
    for file in &manifest.python_files {
        let destino = python_home.join(&file.install_path);
        let parent = destino
            .parent()
            .ok_or_else(|| format!("Ruta sin directorio padre: {}", destino.display()))?;
        fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo crear {}: {e}", parent.display()))?;
        let bytes = copy_android_asset_verified(&file.asset, &destino, &file.sha256)?;
        total = total.saturating_add(bytes);
        if total > MAX_RUNTIME_BYTES {
            return Err(format!(
                "El runtime Python excede el límite de {MAX_RUNTIME_BYTES} bytes"
            ));
        }
    }

    let ytdlp = staging.join("yt-dlp.pyz");
    copy_android_asset_verified(YTDLP_ASSET, &ytdlp, &manifest.ytdlp_sha256)?;
    total = total
        .checked_add(fs::metadata(&ytdlp).map_err(|e| e.to_string())?.len())
        .ok_or_else(|| "Desbordamiento del tamaño del runtime".to_string())?;
    if total > MAX_RUNTIME_BYTES {
        return Err(format!(
            "El runtime Android excede el límite de {MAX_RUNTIME_BYTES} bytes"
        ));
    }
    verify_python(python, &python_home, native_dir, &manifest.python_version)?;
    verify_ytdlp(
        python,
        &python_home,
        native_dir,
        &ytdlp,
        &manifest.ytdlp_version,
    )?;
    sync_directory(&staging);

    // La ruta final está versionada. No se borra una instalación previa: una colisión requiere
    // incrementar runtime_version en el manifiesto y deja intacta la última instalación.
    fs::rename(&staging, runtime_dir).map_err(|e| {
        format!(
            "No se pudo activar el runtime {} sin reemplazar una versión existente: {e}",
            runtime_dir.display()
        )
    })?;
    guard.committed = true;
    Ok(())
}

#[cfg(target_os = "android")]
fn cleanup_staging_dirs(temp_root: &Path) {
    let entries = match fs::read_dir(temp_root) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!(
                "No se pudo inspeccionar el directorio temporal {}: {e}",
                temp_root.display()
            );
            return;
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(".stage-") && entry.path().is_dir() {
            if let Err(e) = fs::remove_dir_all(entry.path()) {
                log::warn!("No se pudo limpiar staging {}: {e}", entry.path().display());
            }
        }
    }
}

#[cfg(target_os = "android")]
fn create_staging_dir(root: &Path) -> Result<PathBuf, String> {
    for _ in 0..32 {
        let id = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = root.join(format!(".stage-{}-{id}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("No se pudo crear staging {}: {e}", path.display())),
        }
    }
    Err("No se pudo crear un directorio staging único".to_string())
}

#[cfg(target_os = "android")]
struct StagingDir {
    path: PathBuf,
    committed: bool,
}

#[cfg(target_os = "android")]
impl Drop for StagingDir {
    fn drop(&mut self) {
        if !self.committed {
            if let Err(e) = fs::remove_dir_all(&self.path) {
                if e.kind() != io::ErrorKind::NotFound {
                    log::warn!("No se pudo limpiar {}: {e}", self.path.display());
                }
            }
        }
    }
}

#[cfg(target_os = "android")]
fn copy_android_asset_verified(
    asset_path: &str,
    destination: &Path,
    expected_hash: &str,
) -> Result<u64, String> {
    let app = ANDROID_APP
        .get()
        .ok_or_else(|| "AssetManager no fue inicializado desde android_main".to_string())?;
    let name = CString::new(asset_path).map_err(|e| format!("Asset inválido: {e}"))?;
    let mut asset = app
        .asset_manager()
        .open(&name)
        .ok_or_else(|| format!("Falta el asset requerido: {asset_path}"))?;
    let declared = asset.length() as u64;
    validate_file_size(declared, expected_hash)
        .map_err(|error| format!("Asset {asset_path}: {error}"))?;

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut output = options
        .open(destination)
        .map_err(|e| format!("No se pudo crear {}: {e}", destination.display()))?;
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = match asset.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Error al leer asset {asset_path}: {e}")),
        };
        total = total.saturating_add(count as u64);
        if total > MAX_FILE_BYTES {
            return Err(format!("El asset {asset_path} excede el límite de tamaño"));
        }
        context.update(&buffer[..count]);
        output
            .write_all(&buffer[..count])
            .map_err(|e| format!("Error al escribir {}: {e}", destination.display()))?;
    }
    if total != declared {
        return Err(format!(
            "Asset truncado {asset_path}: {total} de {declared} bytes"
        ));
    }
    output
        .flush()
        .and_then(|_| output.sync_all())
        .map_err(|e| format!("No se pudo sincronizar {}: {e}", destination.display()))?;
    if let Some(parent) = destination.parent() {
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| format!("No se pudo sincronizar {}: {e}", parent.display()))?;
    }

    let actual = hex(context.finish().as_ref());
    if !actual.eq_ignore_ascii_case(expected_hash) {
        return Err(format!(
            "SHA-256 incorrecto para {asset_path}: se esperaba {expected_hash}, se obtuvo {actual}"
        ));
    }
    Ok(total)
}

#[cfg(target_os = "android")]
fn validate_installed_runtime(
    runtime_dir: &Path,
    manifest: &RuntimeManifest,
) -> Result<(), String> {
    for file in &manifest.python_files {
        let path = runtime_dir.join("python").join(&file.install_path);
        verify_file_hash(&path, &file.sha256)?;
    }
    verify_file_hash(&runtime_dir.join("yt-dlp.pyz"), &manifest.ytdlp_sha256)
}

#[cfg(target_os = "android")]
fn verify_file_hash(path: &Path, expected: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|e| format!("No se pudo inspeccionar {}: {e}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{} no es un archivo regular", path.display()));
    }
    validate_file_size(metadata.len(), expected)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let file = File::open(path).map_err(|e| format!("No se pudo abrir {}: {e}", path.display()))?;
    let actual = sha256_reader(file)?;
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!(
            "SHA-256 incorrecto para {}: esperado {expected}, obtenido {actual}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "android", test))]
fn validate_file_size(size: u64, expected_hash: &str) -> Result<(), String> {
    // CPython contiene __init__.py vacíos: solo se aceptan si el hash los declara vacíos.
    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    if size > MAX_FILE_BYTES {
        return Err(format!("El archivo supera {MAX_FILE_BYTES} bytes"));
    }
    if size == 0 && !expected_hash.eq_ignore_ascii_case(EMPTY_SHA256) {
        return Err("Archivo vacío inesperado".to_string());
    }
    Ok(())
}

#[cfg(target_os = "android")]
fn sha256_reader(mut reader: impl Read) -> Result<String, String> {
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| format!("Error al calcular SHA-256: {e}"))?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
    }
    Ok(hex(context.finish().as_ref()))
}

#[cfg(target_os = "android")]
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

#[cfg(target_os = "android")]
fn validate_python_executable(path: &Path, expected_hash: &str) -> Result<(), String> {
    validate_android_pie(path)?;
    verify_file_hash(path, expected_hash)?;
    let metadata = fs::metadata(path)
        .map_err(|e| format!("No se pudo leer metadata de {}: {e}", path.display()))?;
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(format!(
            "El ejecutable CPython no tiene permisos de ejecución: {}",
            path.display()
        ));
    }
    Ok(())
}

/// Acepta solo ELF64 AArch64 ET_DYN con entrypoint, segmento ejecutable y PT_INTERP Bionic.
/// Una biblioteca compartida ordinaria (e_entry=0) no se confunde con un CLI por su nombre.
#[cfg(target_os = "android")]
fn validate_android_pie(path: &Path) -> Result<(), String> {
    let mut header = [0u8; 4096];
    let mut file = File::open(path)
        .map_err(|e| format!("No se pudo abrir ejecutable {}: {e}", path.display()))?;
    let read = file
        .read(&mut header)
        .map_err(|e| format!("No se pudo leer ELF {}: {e}", path.display()))?;
    let h = &header[..read];
    if h.len() < 64 || &h[..4] != b"\x7fELF" || h[4] != 2 || h[5] != 1 {
        return Err(format!("{} no es ELF64 little-endian", path.display()));
    }
    if u16_le(h, 16) != Some(3) || u16_le(h, 18) != Some(183) {
        return Err(format!(
            "{} no es un PIE AArch64 para Android (ET_DYN/EM_AARCH64)",
            path.display()
        ));
    }
    if u64_le(h, 24).unwrap_or(0) == 0 {
        return Err(format!(
            "{} es una biblioteca ELF sin entrypoint ejecutable",
            path.display()
        ));
    }
    let phoff = u64_le(h, 32).unwrap_or(u64::MAX);
    let phentsize = u16_le(h, 54).unwrap_or(0) as u64;
    let phnum = u16_le(h, 56).unwrap_or(0) as u64;
    if phentsize < 56 || phnum == 0 || phnum > 256 || phoff > 1024 * 1024 {
        return Err(format!(
            "{} tiene una tabla ELF program header inválida",
            path.display()
        ));
    }
    let table_len = phentsize
        .checked_mul(phnum)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("Tabla ELF inválida en {}", path.display()))?;
    let table_start =
        usize::try_from(phoff).map_err(|_| "Offset ELF fuera de rango".to_string())?;
    let table_end = table_start
        .checked_add(table_len)
        .ok_or_else(|| "Tabla ELF fuera de rango".to_string())?;
    if table_end > h.len() {
        file.seek(std::io::SeekFrom::Start(phoff))
            .map_err(|e| format!("No se pudo leer program headers: {e}"))?;
        let mut table = vec![0u8; table_len];
        file.read_exact(&mut table)
            .map_err(|e| format!("Tabla ELF truncada: {e}"))?;
        return check_program_headers(&table, phentsize as usize, phnum as usize, path);
    }
    check_program_headers(
        &h[table_start..table_end],
        phentsize as usize,
        phnum as usize,
        path,
    )
}

#[cfg(target_os = "android")]
fn check_program_headers(
    table: &[u8],
    entsize: usize,
    count: usize,
    path: &Path,
) -> Result<(), String> {
    let mut has_interpreter = false;
    let mut has_executable_segment = false;
    for i in 0..count {
        let entry = &table[i * entsize..(i + 1) * entsize];
        match u32_le(entry, 0) {
            Some(1) if u32_le(entry, 4).unwrap_or(0) & 1 != 0 => {
                has_executable_segment = true;
            }
            Some(3) => {
                let offset = u64_le(entry, 8).unwrap_or(u64::MAX);
                let size = u64_le(entry, 32).unwrap_or(u64::MAX);
                if size == 0 || size > 128 || offset > 1024 * 1024 {
                    return Err(format!("PT_INTERP inválido en {}", path.display()));
                }
                let mut f = File::open(path).map_err(|e| e.to_string())?;
                f.seek(std::io::SeekFrom::Start(offset))
                    .map_err(|e| format!("No se pudo leer PT_INTERP: {e}"))?;
                let mut interp = vec![0u8; size as usize];
                f.read_exact(&mut interp)
                    .map_err(|e| format!("PT_INTERP truncado: {e}"))?;
                while interp.last() == Some(&0) {
                    interp.pop();
                }
                if interp != b"/system/bin/linker64" {
                    return Err(format!(
                        "{} usa un cargador no compatible con Android: {}",
                        path.display(),
                        String::from_utf8_lossy(&interp)
                    ));
                }
                has_interpreter = true;
            }
            _ => {}
        }
    }
    if !has_interpreter || !has_executable_segment {
        return Err(format!(
            "{} no tiene PT_INTERP Bionic o segmento ejecutable",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(target_os = "android")]
fn u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    let b = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

#[cfg(target_os = "android")]
fn u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let b = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

#[cfg(target_os = "android")]
fn u64_le(bytes: &[u8], offset: usize) -> Option<u64> {
    let b = bytes.get(offset..offset.checked_add(8)?)?;
    Some(u64::from_le_bytes([
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
}

#[cfg(target_os = "android")]
fn verify_python(
    python: &Path,
    home: &Path,
    native_dir: &Path,
    expected: &str,
) -> Result<(), String> {
    let output = run_process(python, home, native_dir, &["--version"])?;
    let expected = format!("Python {expected}");
    if output.trim() != expected {
        return Err(format!(
            "CPython devolvió {:?}; se esperaba {expected}",
            output.trim()
        ));
    }
    let implementation = run_process(
        python,
        home,
        native_dir,
        &[
            "-S",
            "-c",
            "import platform, ssl, hashlib, sqlite3, ctypes, zlib; print(platform.python_implementation())",
        ],
    )?;
    if implementation.trim() != "CPython" {
        return Err(format!(
            "El intérprete no es CPython: {:?}",
            implementation.trim()
        ));
    }
    Ok(())
}

#[cfg(target_os = "android")]
fn verify_ytdlp(
    python: &Path,
    home: &Path,
    native_dir: &Path,
    zipapp: &Path,
    expected: &str,
) -> Result<(), String> {
    let zipapp = zipapp.to_string_lossy();
    let output = run_process(python, home, native_dir, &["-S", &zipapp, "--version"])?;
    if output.trim() != expected {
        return Err(format!(
            "yt-dlp devolvió {:?}; se esperaba {}",
            output.trim(),
            expected
        ));
    }
    Ok(())
}

#[cfg(target_os = "android")]
fn run_process(
    python: &Path,
    home: &Path,
    native_dir: &Path,
    args: &[&str],
) -> Result<String, String> {
    let mut command = Command::new(python);
    command
        .args(args)
        .env_clear()
        .env("PYTHONHOME", home)
        .env("PYTHONNOUSERSITE", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("LD_LIBRARY_PATH", native_dir)
        .env("PATH", "/system/bin:/system/xbin")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = crate::runner::instalador::ejecutar_con_timeout(command, PROCESS_TIMEOUT)
        .map_err(|e| format!("No se pudo validar el runtime Python: {e}"))?;
    if !output.estado.success() {
        return Err(format!(
            "El proceso {:?} terminó con {}: {}",
            args.first(),
            output.estado,
            output.stderr.trim()
        ));
    }
    Ok(if output.stdout.trim().is_empty() {
        output.stderr
    } else {
        output.stdout
    })
}

#[cfg(target_os = "android")]
fn validate_ffmpeg_tools(
    native_dir: &Path,
    hash: &str,
    manifest: &RuntimeManifest,
) -> Result<PathBuf, String> {
    let version = manifest
        .ffmpeg_version
        .as_deref()
        .ok_or("Falta ffmpeg_version")?;
    let probe_hash = manifest
        .ffprobe_sha256
        .as_deref()
        .ok_or("Falta ffprobe_sha256")?;
    let ffmpeg = native_dir.join("libffmpeg.so");
    validate_media_tool(&ffmpeg, hash, "ffmpeg", version)?;
    validate_media_tool(
        &native_dir.join("libffprobe.so"),
        probe_hash,
        "ffprobe",
        version,
    )?;
    Ok(ffmpeg)
}

#[cfg(target_os = "android")]
fn validate_media_tool(
    path: &Path,
    expected_hash: &str,
    tool: &str,
    version: &str,
) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!("No se encontró {}", path.display()));
    }
    validate_android_pie(path)?;
    verify_file_hash(path, expected_hash)?;
    if fs::metadata(path)
        .map_err(|e| format!("No se pudo leer metadata: {e}"))?
        .permissions()
        .mode()
        & 0o111
        == 0
    {
        return Err(format!("{tool} no es ejecutable: {}", path.display()));
    }
    let mut command = Command::new(path);
    command
        .arg("-version")
        .env_clear()
        .env(
            "LD_LIBRARY_PATH",
            path.parent().unwrap_or_else(|| Path::new("/system/lib64")),
        )
        .env("PATH", "/system/bin:/system/xbin")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = crate::runner::instalador::ejecutar_con_timeout(command, PROCESS_TIMEOUT)
        .map_err(|e| e.to_string())?;
    let expected = format!("{tool} version {version} ");
    if !output.estado.success()
        || !(output
            .stdout
            .lines()
            .next()
            .is_some_and(|line| line.starts_with(&expected))
            || output
                .stderr
                .lines()
                .next()
                .is_some_and(|line| line.starts_with(&expected)))
    {
        return Err(format!(
            "{tool} no pasó la comprobación -version {version} (estado {})",
            output.estado
        ));
    }
    Ok(())
}

#[cfg(target_os = "android")]
fn install_wrapper(
    path: &Path,
    python: &Path,
    python_home: &Path,
    zipapp: &Path,
    ca_bundle: &Path,
) -> Result<(), String> {
    let contenido = format!(
        "#!/system/bin/sh\nPYTHONHOME={}\nPYTHONNOUSERSITE=1\nPYTHONDONTWRITEBYTECODE=1\nLD_LIBRARY_PATH={}\nSSL_CERT_FILE={}\nexport PYTHONHOME PYTHONNOUSERSITE PYTHONDONTWRITEBYTECODE LD_LIBRARY_PATH SSL_CERT_FILE\nexec {} -S {} --compat-options no-certifi \"$@\"\n",
        shell_quote(&python_home.to_string_lossy()),
        shell_quote(&python.parent().unwrap_or_else(|| Path::new("/system/lib64")).to_string_lossy()),
        shell_quote(&ca_bundle.to_string_lossy()),
        shell_quote(&python.to_string_lossy()),
        shell_quote(&zipapp.to_string_lossy())
    );
    let parent = path
        .parent()
        .ok_or_else(|| format!("El wrapper no tiene directorio padre: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("No se pudo crear {}: {e}", parent.display()))?;
    let temp = parent.join(format!(
        ".yt-dlp-{}-{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("No se pudo crear launcher temporal: {e}"))?;
    if let Err(e) = file
        .write_all(contenido.as_bytes())
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
    {
        let _ = fs::remove_file(&temp);
        return Err(format!("No se pudo escribir el launcher: {e}"));
    }
    drop(file);
    if let Err(e) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(format!(
            "No se pudo activar el launcher {}: {e}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "android", test))]
fn write_ca_bundle(source: &Path, destination: &Path) -> Result<(), String> {
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_BUNDLE: AtomicU64 = AtomicU64::new(0);
    let parent = destination
        .parent()
        .ok_or("El bundle CA no tiene directorio padre")?;
    let temp = parent.join(format!(
        ".android-ca-{}-{}.tmp",
        std::process::id(),
        NEXT_BUNDLE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("No se pudo crear bundle CA temporal: {e}"))?;
    let result = (|| -> Result<(), String> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(source)
            .map_err(|e| format!("No se pudo leer el almacén CA Android: {e}"))?
        {
            let entry = entry.map_err(|e| format!("No se pudo leer una entrada CA: {e}"))?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                return Err(format!("Entrada CA no regular: {}", entry.path().display()));
            }
            paths.push(entry.path());
            if paths.len() > 4096 {
                return Err("El almacén CA supera 4096 certificados".into());
            }
        }
        if paths.is_empty() {
            return Err("El almacén CA Android está vacío".into());
        }
        paths.sort();
        let mut total = 0;
        for path in paths {
            let mut bytes = Vec::new();
            File::open(&path)
                .and_then(|file| file.take(65537).read_to_end(&mut bytes))
                .map_err(|e| format!("No se pudo leer CA {}: {e}", path.display()))?;
            if bytes.len() > 65536
                || !bytes.starts_with(b"-----BEGIN CERTIFICATE-----")
                || !bytes.windows(25).any(|w| w == b"-----END CERTIFICATE-----")
            {
                return Err(format!(
                    "Certificado CA vacío, excesivo o no PEM: {}",
                    path.display()
                ));
            }
            total += bytes.len();
            if total > 8 * 1024 * 1024 {
                return Err("El bundle CA supera 8 MiB".into());
            }
            output
                .write_all(&bytes)
                .and_then(|_| output.write_all(b"\n"))
                .map_err(|e| format!("No se pudo escribir el bundle CA: {e}"))?;
        }
        output
            .flush()
            .and_then(|_| output.sync_all())
            .map_err(|e| format!("No se pudo sincronizar bundle CA: {e}"))?;
        Ok(())
    })();
    drop(output);
    let result = result.and_then(|()| {
        fs::rename(&temp, destination).map_err(|e| format!("No se pudo activar bundle CA: {e}"))
    });
    if result.is_err() {
        if let Err(error) = fs::remove_file(&temp) {
            log::error!("No se pudo limpiar bundle CA temporal: {error}");
        }
    }
    result
}

#[cfg(any(target_os = "android", test))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "android")]
fn sync_directory(path: &Path) {
    if let Err(e) = File::open(path).and_then(|directory| directory.sync_all()) {
        log::warn!("No se pudo sincronizar {}: {e}", path.display());
    }
}

/// Resuelve la URL de un stream en un hilo de aplicación (Android).
#[cfg(target_os = "android")]
pub fn resolve_stream_url(video_url: &str) -> Result<String, String> {
    let yt_dlp = ensure_yt_dlp_executable()?;
    let mut command = crate::runner::ytdlp::crear_comando_ytdlp(&yt_dlp);
    command.args(["-g", "-f", "best"]).arg(video_url);
    let output = crate::runner::instalador::ejecutar_con_timeout(
        command,
        std::time::Duration::from_secs(25),
    )
    .map_err(|e| format!("No se pudo resolver el stream con yt-dlp: {e}"))?;
    if !output.estado.success() {
        return Err(format!(
            "yt-dlp falló con {}: {}",
            output.estado,
            output.stderr.trim()
        ));
    }
    output
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "yt-dlp no devolvió una URL válida: {}",
                output.stderr.trim()
            )
        })
}

#[cfg(target_os = "android")]
pub fn resolve_stream_url_async(video_url: &str) -> mpsc::Receiver<Result<String, String>> {
    let (tx, rx) = mpsc::channel();
    let url = video_url.to_owned();
    std::thread::spawn(move || {
        let _ = tx.send(resolve_stream_url(&url));
    });
    rx
}

#[cfg(not(target_os = "android"))]
pub fn ensure_yt_dlp_executable() -> Result<PathBuf, String> {
    Err("El runtime CPython Android no está disponible fuera de Android".to_string())
}

#[cfg(not(target_os = "android"))]
pub fn resolve_stream_url(_video_url: &str) -> Result<String, String> {
    Err("La resolución de stream con yt-dlp solo está disponible en Android".to_string())
}

#[cfg(not(target_os = "android"))]
pub fn resolve_stream_url_async(_video_url: &str) -> mpsc::Receiver<Result<String, String>> {
    let (tx, rx) = mpsc::channel();
    let _ = tx.send(Err(
        "La resolución de stream con yt-dlp solo está disponible en Android".to_string(),
    ));
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_ca_valida_limites_y_preserva_anterior() {
        use std::fs;
        let root = std::env::temp_dir().join(format!(
            "yt-ca-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("certs");
        fs::create_dir_all(&source).unwrap();
        let destination = root.join("ca.pem");
        fs::write(&destination, b"previous").unwrap();
        assert!(write_ca_bundle(&source, &destination).is_err());
        fs::write(source.join("a.0"), b"").unwrap();
        assert!(write_ca_bundle(&source, &destination).is_err());
        fs::write(source.join("a.0"), vec![b'a'; 65537]).unwrap();
        assert!(write_ca_bundle(&source, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"previous");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_file(&destination).unwrap();
        let pem = b"-----BEGIN CERTIFICATE-----\nfixture\n-----END CERTIFICATE-----\n";
        fs::write(source.join("a.0"), pem).unwrap();
        fs::write(source.join("b.0"), pem).unwrap();
        write_ca_bundle(&source, &destination).unwrap();
        assert_eq!(
            fs::read(&destination).unwrap(),
            [pem.as_slice(), b"\n", pem.as_slice(), b"\n"].concat()
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn manifest_ok() -> RuntimeManifest {
        RuntimeManifest {
            schema_version: 1,
            runtime_version: "cpython-3.12.3-android-arm64-v1".to_string(),
            python_version: "3.12.3".to_string(),
            python_executable_sha256: "a".repeat(64),
            ytdlp_version: "2026.08.19".to_string(),
            ytdlp_sha256: "b".repeat(64),
            python_files: vec![RuntimeFile {
                asset: "android-runtime/python/lib/python3.12/python312.zip".to_string(),
                install_path: "lib/python3.12/python312.zip".to_string(),
                sha256: "c".repeat(64),
            }],
            ffmpeg_sha256: None,
            ffprobe_sha256: None,
            ffmpeg_version: None,
        }
    }

    #[test]
    fn ffmpeg_requiere_par_verificado_y_version() {
        let mut manifest = manifest_ok();
        manifest.ffmpeg_sha256 = Some("d".repeat(64));
        assert!(validate_manifest(&manifest).is_err());
        manifest.ffprobe_sha256 = Some("e".repeat(64));
        manifest.ffmpeg_version = Some("9.0.2".into());
        assert!(validate_manifest(&manifest).is_ok());
        manifest.ffprobe_sha256 = Some("incorrecto".into());
        assert!(validate_manifest(&manifest).is_err());
        manifest.ffprobe_sha256 = Some("e".repeat(64));
        manifest.ffmpeg_version = Some("../9".into());
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn manifest_rechaza_versiones_y_rutas_invalidas() {
        assert!(validate_manifest(&manifest_ok()).is_ok());

        let mut manifest = manifest_ok();
        manifest.runtime_version = "../outside".to_string();
        assert!(validate_manifest(&manifest).is_err());

        let mut manifest = manifest_ok();
        manifest.python_files[0].install_path = "../../outside".to_string();
        assert!(validate_manifest(&manifest).is_err());

        let mut manifest = manifest_ok();
        manifest.python_files[0].asset = "android-runtime/../../outside".to_string();
        assert!(validate_manifest(&manifest).is_err());

        let mut manifest = manifest_ok();
        manifest.python_executable_sha256 = "not-a-hash".to_string();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn manifiesto_serde_valida_schema_y_assets() {
        let json = format!(
            r#"{{
                "schema_version": 1,
                "runtime_version": "cpython-3.12.3-android-arm64-v1",
                "python_version": "3.12.3",
                "python_executable_sha256": "{}",
                "ytdlp_version": "2026.08.19",
                "ytdlp_sha256": "{}",
                "python_files": [{{
                    "asset": "android-runtime/python/lib/python3.12/python312.zip",
                    "install_path": "lib/python3.12/python312.zip",
                    "sha256": "{}"
                }}],
                "ffmpeg_sha256": null
            }}"#,
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
        );
        let manifest: RuntimeManifest = serde_json::from_str(&json).expect("manifest JSON");
        assert!(validate_manifest(&manifest).is_ok());
    }

    #[test]
    fn manifiesto_expandido_requiere_os_y_encodings() {
        let mut manifest = manifest_ok();
        manifest.python_files = ["os.py", "encodings/__init__.py"]
            .iter()
            .map(|module| RuntimeFile {
                asset: format!("android-runtime/python/lib/python3.12/{module}"),
                install_path: format!("lib/python3.12/{module}"),
                sha256: "c".repeat(64),
            })
            .collect();
        assert!(validate_manifest(&manifest).is_ok());
        manifest.python_files.pop();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn archivos_vacios_solo_si_el_hash_lo_declara() {
        assert!(validate_file_size(
            0,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        )
        .is_ok());
        assert!(validate_file_size(0, &"a".repeat(64)).is_err());
        assert!(validate_file_size(MAX_FILE_BYTES, &"a".repeat(64)).is_ok());
        assert!(validate_file_size(MAX_FILE_BYTES + 1, &"a".repeat(64)).is_err());
    }

    #[test]
    fn manifiesto_empaquetado_cumple_el_contrato() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(root.join("assets/android-runtime/manifest.json"))
            .expect("manifiesto Android generado");
        assert!(bytes.len() <= MAX_MANIFEST_BYTES);
        let manifest: RuntimeManifest = serde_json::from_slice(&bytes).expect("JSON válido");
        validate_manifest(&manifest).expect("layout válido");
        let mut paths = vec![
            (
                root.join("jniLibs/arm64-v8a/libpython3.so"),
                manifest.python_executable_sha256,
            ),
            (
                root.join("assets/android-runtime/yt-dlp.pyz"),
                manifest.ytdlp_sha256,
            ),
        ];
        if let Some(hash) = manifest.ffmpeg_sha256 {
            paths.push((root.join("jniLibs/arm64-v8a/libffmpeg.so"), hash));
        }
        if let Some(hash) = manifest.ffprobe_sha256 {
            paths.push((root.join("jniLibs/arm64-v8a/libffprobe.so"), hash));
        }
        paths.extend(
            manifest
                .python_files
                .into_iter()
                .map(|file| (root.join("assets").join(file.asset), file.sha256)),
        );
        for (path, expected) in paths {
            let bytes = std::fs::read(&path).expect("artefacto empaquetado");
            validate_file_size(bytes.len() as u64, &expected).expect("tamaño válido");
            let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
            let actual: String = digest
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(actual, expected, "{}", path.display());
        }
    }

    #[test]
    fn shell_launcher_escapa_rutas_y_preserva_argumentos() {
        assert_eq!(shell_quote("/data/app/a'b"), "'/data/app/a'\\''b'");
        let script = format!(
            "exec {} -S {} \"$@\"",
            shell_quote("/app/lib/libpython3.so"),
            shell_quote("/app/files/yt-dlp.pyz")
        );
        assert!(script.ends_with("\"$@\""));
        assert!(script.contains("libpython3.so"));
    }

    #[test]
    fn versiones_python_exigen_tres_componentes_numericos() {
        assert!(is_version("3.12.3", 3));
        assert!(is_version("2026.8.19", 3));
        assert!(!is_version("3.12", 3));
        assert!(!is_version("3.x.3", 3));
        assert!(!is_version("3.12.3-dev", 3));
    }
}
