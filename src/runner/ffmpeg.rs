use std::path::PathBuf;
use std::process::Command;

pub fn inyectar_ffmpeg(cmd: &mut Command, ffmpeg_path: &Option<PathBuf>) {
    if let Some(ref path) = ffmpeg_path {
        #[cfg(target_os = "android")]
        {
            // En Android este path solo se devuelve si el .so empaqueta un ffmpeg PIE validado.
            cmd.arg("--ffmpeg-location").arg(path);
        }

        #[cfg(not(target_os = "android"))]
        {
            if let Some(parent) = path.parent() {
                cmd.arg("--ffmpeg-location").arg(parent);
            } else {
                cmd.arg("--ffmpeg-location").arg(path);
            }
        }
    }
}
