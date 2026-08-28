use std::path::PathBuf;
use std::process::Command;
use super::ffmpeg::inyectar_ffmpeg;

pub struct YtDlpCommand {
    binary_path: PathBuf,
    ffmpeg_path: Option<PathBuf>,
}

impl YtDlpCommand {
    pub fn new(binary_path: PathBuf, ffmpeg_path: Option<PathBuf>) -> Self {
        Self { binary_path, ffmpeg_path }
    }

    pub fn descargar_audio(&self, url: &str, output_dir: &str) -> Command {
        let mut cmd = Command::new(&self.binary_path);
        
        cmd.arg("-x")
           .arg("--audio-format").arg("mp3")
           .arg("--no-playlist")
           .arg("--no-warnings") // Silencia experimentos inofensivos de YouTube
           .arg("--restrict-filenames")
           .arg("--extractor-args").arg("youtube:player_client=android")
           .arg("-f").arg("ba/b")
           .arg("-o").arg(format!("{}/%(title)s.%(ext)s", output_dir))
           .arg(url);

        inyectar_ffmpeg(&mut cmd, &self.ffmpeg_path);
        cmd
    }

    pub fn descargar_video(&self, url: &str, output_dir: &str) -> Command {
        let mut cmd = Command::new(&self.binary_path);
        
        cmd.arg("-f").arg("bv*+ba/b")
           .arg("--no-playlist")
           .arg("--no-warnings")
           .arg("--restrict-filenames")
           .arg("--extractor-args").arg("youtube:player_client=android")
           .arg("-o").arg(format!("{}/%(title)s.%(ext)s", output_dir))
           .arg(url);

        inyectar_ffmpeg(&mut cmd, &self.ffmpeg_path);
        cmd
    }
}