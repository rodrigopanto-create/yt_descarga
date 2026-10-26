#[cfg(all(target_os = "android", feature = "mpv"))]
use std::ffi::CString;

#[cfg(all(target_os = "android", feature = "mpv"))]
pub fn load_stream_in_mpv(
    mpv_handle: *mut std::ffi::c_void,
    stream_url: &str,
) -> Result<(), String> {
    if mpv_handle.is_null() {
        return Err("El handle de libmpv es nulo".to_string());
    }

    unsafe {
        let command = CString::new("loadfile").map_err(|e| format!("CString cmd: {e}"))?;
        let url = CString::new(stream_url).map_err(|e| format!("CString url: {e}"))?;

        let mut argv = [command.as_ptr(), url.as_ptr(), std::ptr::null()];

        let rc = libmpv_sys::mpv_command(mpv_handle.cast(), argv.as_mut_ptr());

        if rc < 0 {
            return Err(format!("libmpv rechazó loadfile: {rc}"));
        }

        log::info!("URL enviada a mpv: {stream_url}");
        Ok(())
    }
}

#[cfg(not(all(target_os = "android", feature = "mpv")))]
pub fn load_stream_in_mpv(
    _mpv_handle: *mut std::ffi::c_void,
    stream_url: &str,
) -> Result<(), String> {
    log::info!("MPV no está habilitado en esta compilación; URL preparada: {stream_url}");
    Ok(())
}

#[cfg(target_os = "android")]
pub fn play_youtube_stream(video_id: &str) -> Result<String, String> {
    let url = format!("https://www.youtube.com/watch?v={video_id}");
    log::info!("Resolviendo stream para: {url}");

    let stream_url = crate::android_yt_dlp::resolve_stream_url(&url).map_err(|e| {
        log::error!("Fallo al resolver stream con yt-dlp: {e}");
        e
    })?;

    log::info!("URL resuelta por yt-dlp: {stream_url}");
    Ok(stream_url)
}

#[cfg(target_os = "android")]
pub fn play_youtube_stream_in_mpv(
    mpv_handle: *mut std::ffi::c_void,
    video_id: &str,
) -> Result<(), String> {
    let stream_url = play_youtube_stream(video_id)?;
    load_stream_in_mpv(mpv_handle, &stream_url)
}
