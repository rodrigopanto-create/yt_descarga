pub mod app;
pub mod downloader;
pub mod extractor;
pub mod platform;
pub mod runner;

pub use app::YtApp;

// ... el resto de tu android_main se queda exactamente igual ...

// ==========================================
// CÓDIGO EXCLUSIVO PARA COMPILACIÓN EN ANDROID
// ==========================================
#[cfg(target_os = "android")]
use winit::platform::android::EventLoopBuilderExtAndroid;

#[cfg(target_os = "android")]
#[no_mangle]
pub fn android_main(app: android_activity::AndroidApp) {
    use eframe::NativeOptions;

    let mut options = NativeOptions::default();
    
    // Forma correcta en eframe 0.27+
    options.event_loop_builder = Some(Box::new(move |builder| {
        builder.with_android_app(app);
    }));

    let _ = eframe::run_native(
        "YouTube Downloader Portable",
        options,
        Box::new(|cc| Box::new(YtApp::new(cc))),
    );
}