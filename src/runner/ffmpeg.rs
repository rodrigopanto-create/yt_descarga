use std::path::PathBuf;
use std::process::Command;

pub fn inyectar_ffmpeg(cmd: &mut Command, ffmpeg_path: &Option<PathBuf>) {
    if let Some(ref path) = ffmpeg_path {
        if let Some(parent) = path.parent() {
            cmd.arg("--ffmpeg-location").arg(parent);
        } else {
            cmd.arg("--ffmpeg-location").arg(path);
        }
    }
}