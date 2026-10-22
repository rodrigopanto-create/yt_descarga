use super::ffmpeg::inyectar_ffmpeg;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct YtDlpCommand {
    binary_path: PathBuf,
    ffmpeg_path: Option<PathBuf>,
}

/// Prepara el comando base adaptado a la plataforma.
/// En Android, invoca el intérprete a través de /system/bin/sh para respetar W^X.
pub fn crear_comando_ytdlp(ytdlp_path: &Path) -> Command {
    #[cfg(target_os = "android")]
    {
        let mut cmd = Command::new("/system/bin/sh");
        cmd.arg(ytdlp_path);
        cmd
    }

    #[cfg(not(target_os = "android"))]
    {
        Command::new(ytdlp_path)
    }
}

impl YtDlpCommand {
    pub fn new(binary_path: PathBuf, ffmpeg_path: Option<PathBuf>) -> Self {
        Self {
            binary_path,
            ffmpeg_path,
        }
    }

    /// Configura y retorna el Command para extracción de audio (MP3).
    pub fn descargar_audio(&self, url: &str, output_dir: &str) -> Command {
        let mut cmd = crear_comando_ytdlp(&self.binary_path);

        cmd.arg("-x")
            .arg("--audio-format")
            .arg("mp3")
            .arg("--audio-quality")
            .arg("0") // Máxima calidad VBR
            .arg("--no-playlist")
            .arg("--no-warnings")
            .arg("--restrict-filenames")
            .arg("--newline") // Esencial para el parsing en tiempo real del mpsc en downloader.rs
            .arg("--extractor-args")
            .arg("youtube:player_client=android")
            .arg("-f")
            .arg("ba/b")
            .arg("-o")
            .arg(format!("{}/%(title)s.%(ext)s", output_dir))
            .arg(url);

        inyectar_ffmpeg(&mut cmd, &self.ffmpeg_path);
        cmd
    }

    /// Configura y retorna el Command para descarga de video combinando mejor video + audio.
    pub fn descargar_video(&self, url: &str, output_dir: &str) -> Command {
        let mut cmd = crear_comando_ytdlp(&self.binary_path);

        cmd.arg("-f")
            .arg("bv*+ba/b")
            .arg("--no-playlist")
            .arg("--no-warnings")
            .arg("--restrict-filenames")
            .arg("--newline") // Vital para el flujo de logs hacia la UI
            .arg("--extractor-args")
            .arg("youtube:player_client=android")
            .arg("-o")
            .arg(format!("{}/%(title)s.%(ext)s", output_dir))
            .arg(url);

        inyectar_ffmpeg(&mut cmd, &self.ffmpeg_path);
        cmd
    }

    /// Método genérico para comandos personalizados o metadatos si se requiere expandir.
    pub fn obtener_info(&self, url: &str) -> Command {
        let mut cmd = crear_comando_ytdlp(&self.binary_path);

        cmd.arg("--dump-json")
            .arg("--no-playlist")
            .arg("--extractor-args")
            .arg("youtube:player_client=android")
            .arg(url);

        cmd
    }
}
