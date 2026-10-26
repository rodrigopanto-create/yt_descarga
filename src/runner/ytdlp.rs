use super::ffmpeg::inyectar_ffmpeg;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct YtDlpCommand {
    binary_path: PathBuf,
    ffmpeg_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_inyecta_el_ejecutable_android_sin_perder_argumentos() {
        let runner = YtDlpCommand::new(
            PathBuf::from("/app/files/bin/yt-dlp"),
            Some(PathBuf::from("/app/lib/libffmpeg.so")),
        );
        let command = runner.descargar_audio("https://example.org/audio", "/app/files/downloads");
        assert_eq!(command.get_program(), "/system/bin/sh");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert_eq!(args[0], "/app/files/bin/yt-dlp");
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--audio-format", "mp3"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--ffmpeg-location", "/app/lib/libffmpeg.so"]));
        assert!(args.contains(&"https://example.org/audio"));
    }

    #[test]
    fn video_sin_ffmpeg_conserva_el_fallback_existente() {
        let runner = YtDlpCommand::new(PathBuf::from("/app/files/bin/yt-dlp"), None);
        let command = runner.descargar_video("https://example.org/video", "/app/files/downloads");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["-f", "bv*+ba/b"]));
        assert!(!args.contains(&"--ffmpeg-location"));
    }
}

/// Invoca el wrapper mediante `/system/bin/sh` para respetar las restricciones W^X.
pub fn crear_comando_ytdlp(ytdlp_path: &Path) -> Command {
    let mut cmd = Command::new("/system/bin/sh");
    cmd.arg(ytdlp_path);
    cmd
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
