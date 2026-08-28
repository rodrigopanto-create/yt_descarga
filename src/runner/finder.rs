use std::path::PathBuf;
use std::process::Command;
use std::fs::{self, File};
use std::io::copy;

pub struct BinaryPaths {
    pub ytdlp: PathBuf,
    pub ffmpeg: Option<PathBuf>,
}

pub fn resolver_binarios() -> Result<BinaryPaths, String> {
    // 1. Buscar en el PATH global del sistema (pip/sistema)
    if let Ok(ytdlp_path) = buscar_en_path("yt-dlp") {
        let ffmpeg_path = buscar_en_path("ffmpeg").ok();
        return Ok(BinaryPaths { ytdlp: ytdlp_path, ffmpeg: ffmpeg_path });
    }

    // 2. Si la versión es "Full", extraer y usar los binarios embebidos
    #[cfg(feature = "full-binaries")]
    {
        if let Ok(paths) = extraer_binarios_embebidos() {
            return Ok(paths);
        }
    }

    // 3. Buscar binarios locales previa/manualmente guardados en bin/
    let local_ytdlp = if cfg!(target_os = "windows") {
        PathBuf::from("bin/yt-dlp.exe")
    } else {
        PathBuf::from("bin/yt-dlp")
    };

    if local_ytdlp.exists() {
        let local_ffmpeg = if cfg!(target_os = "windows") {
            PathBuf::from("bin/ffmpeg.exe")
        } else {
            PathBuf::from("bin/ffmpeg")
        };
        let ffmpeg_path = if local_ffmpeg.exists() { Some(local_ffmpeg) } else { None };
        return Ok(BinaryPaths { ytdlp: local_ytdlp, ffmpeg: ffmpeg_path });
    }

    // 4. Fallback: Descarga directa desde GitHub
    println!("[+] Binarios no encontrados. Descargando versión más reciente...");
    descargar_fallback_github()
}

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

#[cfg(feature = "full-binaries")]
fn extraer_binarios_embebidos() -> Result<BinaryPaths, String> {
    let target_dir = PathBuf::from("bin");
    fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

    // Embed de los ejecutables según el SO en tiempo de compilación
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
        ffmpeg: None, // ffmpeg se puede embeber con include_bytes! de forma análoga
    })
}

fn descargar_fallback_github() -> Result<BinaryPaths, String> {
    let target_dir = PathBuf::from("bin");
    fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

    let (url, filename) = if cfg!(target_os = "windows") {
        ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe", "yt-dlp.exe")
    } else {
        ("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp", "yt-dlp")
    };

    let dest_path = target_dir.join(filename);

    if !dest_path.exists() {
        let mut response = reqwest::blocking::get(url).map_err(|e| format!("Error de red: {}", e))?;
        let mut out_file = File::create(&dest_path).map_err(|e| e.to_string())?;
        copy(&mut response, &mut out_file).map_err(|e| e.to_string())?;

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