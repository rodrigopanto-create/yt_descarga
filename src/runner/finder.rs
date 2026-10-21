// =========================================================================
// CÓDIGO SOLO PARA NATIVO (Windows, Linux, Android)
// =========================================================================
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
use std::path::Path;
use std::path::PathBuf;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
use std::sync::{Mutex, PoisonError};

#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(target_os = "android"))]
use super::instalador;

pub struct BinaryPaths {
    pub ytdlp: PathBuf,
    pub ffmpeg: Option<PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn resolver_binarios() -> Result<BinaryPaths, String> {
    #[cfg(target_os = "android")]
    {
        resolver_android()
    }

    #[cfg(not(target_os = "android"))]
    {
        resolver_desktop()
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn resolver_desktop() -> Result<BinaryPaths, String> {
    let mut errores = Vec::new();
    let mut candidatos = Vec::new();

    let legacy_bin = PathBuf::from("bin");
    let managed_bin = dir_binarios_gestionados_desktop();

    if let Ok(path) = which::which("yt-dlp") {
        candidatos.push(path);
    }
    candidatos.push(managed_bin.join(nombre_ytdlp()));
    candidatos.push(legacy_bin.join(nombre_ytdlp()));

    for candidato in candidatos {
        if !candidato.is_file() {
            continue;
        }
        match instalador::validar_ytdlp_existente(&candidato) {
            Ok(()) => {
                let ffmpeg = ffmpeg_companero(candidato.parent())
                    .or_else(|| which::which("ffmpeg").ok())
                    .or_else(|| ffmpeg_companero(Some(&managed_bin)))
                    .or_else(|| ffmpeg_companero(Some(&legacy_bin)));
                return Ok(BinaryPaths {
                    ytdlp: candidato,
                    ffmpeg,
                });
            }
            Err(e) => errores.push(format!("{}: {e}", candidato.display())),
        }
    }

    println!(
        "[+] yt-dlp no disponible o no válido. Descargando versión verificada en {}...",
        managed_bin.display()
    );
    descargar_fallback_github(&managed_bin).map_err(|e| {
        if errores.is_empty() {
            format!("No fue posible resolver yt-dlp: {e}")
        } else {
            format!(
                "No fue posible resolver yt-dlp. Candidatos inválidos: {}. Fallback: {e}",
                errores.join(" | ")
            )
        }
    })
}

#[cfg(target_os = "android")]
fn resolver_android() -> Result<BinaryPaths, String> {
    let tools = crate::android_yt_dlp::prepare_android_tools()?;
    Ok(BinaryPaths {
        ytdlp: tools.ytdlp,
        ffmpeg: tools.ffmpeg,
    })
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn nombre_ytdlp() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn nombre_ffmpeg() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn ffmpeg_companero(dir: Option<&Path>) -> Option<PathBuf> {
    let path = dir?.join(nombre_ffmpeg());
    path.is_file().then_some(path)
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn dir_binarios_gestionados_desktop() -> PathBuf {
    if let Some(base) = dirs::data_local_dir() {
        base.join("yt_descarga").join("bin")
    } else {
        PathBuf::from("bin")
    }
}

/// Descarga yt-dlp desde GitHub Releases y lo instala en `target_dir`.
///
/// Solo devuelve `Ok` cuando el binario ha superado la verificación SHA-256 contra el
/// `SHA2-256SUMS` de la misma release, la validación de formato/arquitectura y la ejecución
/// de `--version`, y ha sido movido atómicamente a su ruta final (ver `instalador`).
/// Un mutex serializa los intentos dentro del proceso; entre procesos, cada intento usa su
/// propio temporal y el `rename` final garantiza que el destino solo contiene binarios validados.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn descargar_fallback_github(target_dir: &Path) -> Result<BinaryPaths, String> {
    static INSTALACION: Mutex<()> = Mutex::new(());
    // El mutex no protege datos: un pánico previo no deja estado inconsistente.
    let _guardia = INSTALACION.lock().unwrap_or_else(PoisonError::into_inner);

    let solicitud = instalador::solicitud_ytdlp(target_dir).map_err(|e| e.to_string())?;

    // Otro hilo pudo completar la instalación mientras esperábamos el mutex.
    let existente = target_dir.join(solicitud.artefacto.destino);
    let ytdlp = if existente.is_file() {
        match instalador::validar_ytdlp_existente(&existente) {
            Ok(()) => existente,
            Err(_) => instalador::instalar_desde_release(&solicitud).map_err(|e| e.to_string())?,
        }
    } else {
        instalador::instalar_desde_release(&solicitud).map_err(|e| e.to_string())?
    };

    Ok(BinaryPaths {
        ytdlp,
        ffmpeg: ffmpeg_companero(Some(target_dir)).or_else(|| which::which("ffmpeg").ok()),
    })
}

// =========================================================================
// CÓDIGO DUMMY PARA WEBASSEMBLY (Navegador)
// =========================================================================
#[cfg(target_arch = "wasm32")]
pub fn resolver_binarios() -> Result<BinaryPaths, String> {
    Ok(BinaryPaths {
        ytdlp: PathBuf::from("web_yt_dlp"),
        ffmpeg: None,
    })
}
