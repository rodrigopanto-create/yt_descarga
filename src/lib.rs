#[cfg(target_os = "android")]
mod android_downloads;
pub mod android_yt_dlp;
pub mod app;
pub mod downloader;
pub mod extractor;
pub mod mpv_bridge;
pub mod mpv_controller;
pub mod platform;
pub mod runner;

pub use app::YtApp;

// ==========================================
// CÓDIGO EXCLUSIVO PARA COMPILACIÓN EN ANDROID
// ==========================================
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
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

    let _ = eframe::run_native(
        "YouTube Downloader Portable",
        options,
        Box::new(|cc| Box::new(YtApp::new(cc))),
    );
}

// ==========================================
// CÓDIGO EXCLUSIVO PARA COMPILACIÓN EN WEBASSEMBLY
// ==========================================
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async {
        eframe::WebRunner::new()
            .start(
                "the_canvas_id",
                web_options,
                Box::new(|cc| Box::new(YtApp::new(cc))),
            )
            .await
            .expect("Error al iniciar eframe en WebAssembly");
    });

    Ok(())
}