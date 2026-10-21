// =========================================================================
// CÓDIGO SOLO PARA NATIVO (Windows, Linux, Android)
// Se compila únicamente cuando NO estamos generando la versión para el navegador
// =========================================================================
#[cfg(not(target_arch = "wasm32"))]
use std::fs::{self, File};
#[cfg(not(target_arch = "wasm32"))]
use std::io::copy;
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::process::Command;

// Estructura compartida (tanto en escritorio como en la web para no romper llamadas)
pub struct BinaryPaths {
    pub ytdlp: PathBuf,
    pub ffmpeg: Option<PathBuf>,
}

// -------------------------------------------------------------------------
// IMPLEMENTACIÓN NATIVA (Manejo de procesos, descargas de GitHub y sistema de archivos)
// -------------------------------------------------------------------------
#[cfg(not(target_arch = "wasm32"))]
pub fn resolver_binarios() -> Result<BinaryPaths, String> {
    let target_dir = obtener_directorio_trabajo();

    // 1. Buscar en el PATH global del sistema (pip/sistema)
    if let Ok(ytdlp_path) = buscar_en_path("yt-dlp") {
        let ffmpeg_path = buscar_en_path("ffmpeg").ok();
        return Ok(BinaryPaths { ytdlp: ytdlp_path, ffmpeg: ffmpeg_path });
    }

    // 2. Si la versión es "Full", extraer y usar los binarios embebidos
    #[cfg(feature = "full-binaries")]
    {
        if let Ok(paths) = extraer_binarios_embebidos(&target_dir) {
            return Ok(paths);
        }
    }

    // 3. Buscar binarios locales guardados previamente en el directorio de trabajo
    let local_ytdlp = if cfg!(target_os = "windows") {
        target_dir.join("yt-dlp.exe")
    } else {
        target_dir.join("yt-dlp")
    };

    if local_ytdlp.exists() {
        let local_ffmpeg = if cfg!(target_os = "windows") {
            target_dir.join("ffmpeg.exe")
        } else {
            target_dir.join("ffmpeg")
        };
        let ffmpeg_path = if local_ffmpeg.exists() { Some(local_ffmpeg) } else { None };
        return Ok(BinaryPaths { ytdlp: local_ytdlp, ffmpeg: ffmpeg_path });
    }

    // 4. Fallback: Descarga directa desde GitHub a la ruta con permisos
    println!("[+] Binarios no encontrados. Descargando versión más reciente...");
    descargar_fallback_github(&target_dir)
}

#[cfg(not(target_arch = "wasm32"))]
fn buscar_en_path(binary_name: &str) -> Result<PathBuf, String> {
    let cmd_name = if cfg!(target_os = "windows") { "where" } else { "which" };
    let output = Command::new(cmd_name).arg(binary_name).output();
    
    if let Ok(out) = output {
        if out.status.success() {
            let path_str = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path_str.is_empty() {
                return Ok(PathBuf::from(path_str));
            }
        }
    }
    Err(format!("{} no disponible en PATH", binary_name))
}

#[cfg(not(target_arch = "wasm32"))]
fn obtener_directorio_trabajo() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        // Ruta del sandbox nativo de Android con permisos de escritura y ejecución
        let android_files_dir = PathBuf::from("/data/data/rust.yt_descarga_lib/files");
        if !android_files_dir.exists() {
            let _ = fs::create_dir_all(&android_files_dir);
        }
        android_files_dir
    }
    #[cfg(not(target_os = "android"))]
    {
        PathBuf::from("bin")
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(feature = "full-binaries")]
fn extraer_binarios_embebidos(target_dir: &PathBuf) -> Result<BinaryPaths, String> {
    fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    let ytdlp_bytes = include_bytes!("../../bin/yt-dlp.exe");
    #[cfg(not(target_os = "windows"))]
    let ytdlp_bytes = include_bytes!("../../bin/yt-dlp");

    let ytdlp_path = if cfg!(target_os = "windows") {
        target_dir.join("yt-dlp.exe")
    } else {
        target_dir.join("yt-dlp")
    };

    if !ytdlp_path.exists() {
        fs::write(&ytdlp_path, ytdlp_bytes).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&ytdlp_path).map_err(|e| e.to_string())?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&ytdlp_path, perms).map_err(|e| e.to_string())?;
        }
    }

    Ok(BinaryPaths {
        ytdlp: ytdlp_path,
        ffmpeg: None,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn descargar_fallback_github(target_dir: &PathBuf) -> Result<BinaryPaths, String> {
    fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;

    let (url, filename) = if cfg!(target_os = "windows") {
        ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe", "yt-dlp.exe")
    } else {
        ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp", "yt-dlp")
    };

    let dest_path = target_dir.join(filename);

    if !dest_path.exists() {
        let mut response = reqwest::blocking::get(url).map_err(|e| format!("Error de red: {}", e))?;
        let mut out_file = File::create(&dest_path).map_err(|e| format!("Error al crear archivo: {}", e))?;
        copy(&mut response, &mut out_file).map_err(|e| format!("Error al copiar: {}", e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest_path).map_err(|e| e.to_string())?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dest_path, perms).map_err(|e| e.to_string())?;
        }
    }

    Ok(BinaryPaths {
        ytdlp: dest_path,
        ffmpeg: None,
    })
}

// =========================================================================
// CÓDIGO DUMMY PARA WEBASSEMBLY (Navegador)
// Evita buscar ejecutables del sistema o hacer peticiones HTTP síncronas
// =========================================================================
#[cfg(target_arch = "wasm32")]
pub fn resolver_binarios() -> Result<BinaryPaths, String> {
    // Retornamos rutas vacías/simuladas porque el navegador no ejecuta binarios nativos directamente
    Ok(BinaryPaths {
        ytdlp: PathBuf::from("web_yt_dlp"),
        ffmpeg: None,
    })
}