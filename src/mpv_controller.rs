#[cfg(all(target_os = "android", feature = "mpv"))]
use std::ffi::CString;

#[cfg(all(target_os = "android", feature = "mpv"))]
pub struct MpvController {
    handle: *mut std::ffi::c_void,
}

#[cfg(all(target_os = "android", feature = "mpv"))]
impl MpvController {
    pub fn new() -> Result<Self, String> {
        let handle = unsafe { libmpv_sys::mpv_create() };
        if handle.is_null() {
            return Err("libmpv no pudo crear el contexto".to_string());
        }

        let handle = handle.cast::<std::ffi::c_void>();
        set_option(handle, "vo", "gpu")?;
        set_option(handle, "hwdec", "auto-safe")?;

        let result = unsafe { libmpv_sys::mpv_initialize(handle.cast()) };
        if result < 0 {
            unsafe { libmpv_sys::mpv_terminate_destroy(handle.cast()) };
            return Err(format!("libmpv no pudo inicializarse: {result}"));
        }

        log::info!("Contexto libmpv inicializado");
        Ok(Self { handle })
    }

    pub fn loadfile(&self, stream_url: &str) -> Result<(), String> {
        if stream_url.is_empty() {
            return Err("La URL del stream está vacía".to_string());
        }

        let command = CString::new("loadfile").map_err(|e| format!("CString cmd: {e}"))?;
        let url = CString::new(stream_url).map_err(|e| format!("CString url: {e}"))?;
        let mut argv = [command.as_ptr(), url.as_ptr(), std::ptr::null()];

        let result = unsafe { libmpv_sys::mpv_command(self.handle.cast(), argv.as_mut_ptr()) };

        if result < 0 {
            return Err(format!("libmpv rechazó loadfile: {result}"));
        }

        log::info!("Stream cargado en libmpv");
        Ok(())
    }

    pub fn play_youtube(&self, video_id: &str) -> Result<(), String> {
        if video_id.trim().is_empty() {
            return Err("El video_id de YouTube está vacío".to_string());
        }

        let stream_url = crate::android_yt_dlp::resolve_stream_url(&format!(
            "https://www.youtube.com/watch?v={}",
            video_id.trim()
        ))?;

        self.loadfile(&stream_url)
    }

    pub fn stop(&self) -> Result<(), String> {
        self.command("stop")
    }

    fn command(&self, name: &str) -> Result<(), String> {
        let command = CString::new(name).map_err(|e| format!("CString cmd: {e}"))?;
        let mut argv = [command.as_ptr(), std::ptr::null()];
        let result = unsafe { libmpv_sys::mpv_command(self.handle.cast(), argv.as_mut_ptr()) };

        if result < 0 {
            return Err(format!("libmpv rechazó {name}: {result}"));
        }
        Ok(())
    }

    pub fn raw_handle(&self) -> *mut std::ffi::c_void {
        self.handle
    }
}

#[cfg(all(target_os = "android", feature = "mpv"))]
fn set_option(handle: *mut std::ffi::c_void, name: &str, value: &str) -> Result<(), String> {
    let name = CString::new(name).map_err(|e| format!("CString option: {e}"))?;
    let value = CString::new(value).map_err(|e| format!("CString value: {e}"))?;
    let result =
        unsafe { libmpv_sys::mpv_set_option_string(handle.cast(), name.as_ptr(), value.as_ptr()) };

    if result < 0 {
        return Err(format!("No se pudo configurar MPV {name:?}: {result}"));
    }
    Ok(())
}

#[cfg(all(target_os = "android", feature = "mpv"))]
impl Drop for MpvController {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { libmpv_sys::mpv_terminate_destroy(self.handle.cast()) };
            log::info!("Contexto libmpv destruido");
        }
    }
}

#[cfg(not(all(target_os = "android", feature = "mpv")))]
pub struct MpvController;

#[cfg(not(all(target_os = "android", feature = "mpv")))]
impl MpvController {
    pub fn new() -> Result<Self, String> {
        Err("MpvController requiere Android y la feature 'mpv'".to_string())
    }
}
