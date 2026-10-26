use std::path::PathBuf;

pub struct BinaryPaths {
    pub ytdlp: PathBuf,
    pub ffmpeg: Option<PathBuf>,
}

pub fn resolver_binarios() -> Result<BinaryPaths, String> {
    let tools = crate::android_yt_dlp::prepare_android_tools()?;
    Ok(BinaryPaths {
        ytdlp: tools.ytdlp,
        ffmpeg: tools.ffmpeg,
    })
}
