pub mod android_yt_dlp;
pub mod app;
pub mod downloader;
pub mod extractor;
pub mod mpv_bridge;
pub mod mpv_controller;
pub mod platform;
pub mod runner;

pub use app::YtApp;

use winit::platform::android::activity::AndroidApp;

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("YT_DESCARGA_RUST"),
    );

    if let Err(error) = android_yt_dlp::initialize_android_assets(app.clone()) {
        log::error!("No se pudo inicializar AssetManager: {error}");
    }

    log::info!("Iniciando aplicación en Android...");

    let mut options = eframe::NativeOptions::default();

    // Configuración explícita de renderizado para evitar pánico en GPUs Mali/PowerVR
    options.renderer = eframe::Renderer::Glow;

    options.event_loop_builder = Some(Box::new(move |builder| {
        use winit::platform::android::EventLoopBuilderExtAndroid;
        builder.with_android_app(app);
    }));

    if let Err(error) = eframe::run_native(
        "YouTube Downloader Portable",
        options,
        Box::new(|cc| Box::new(YtApp::new(cc))),
    ) {
        log::error!("No se pudo iniciar la interfaz Android: {error}");
    }
}
