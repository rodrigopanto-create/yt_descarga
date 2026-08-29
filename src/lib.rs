pub mod app;
pub mod downloader;
pub mod extractor;
pub mod platform;
pub mod runner;

pub use app::YtApp;

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
    
    options.event_loop_builder = Some(Box::new(move |builder| {
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