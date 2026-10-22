use eframe::egui;
use yt_descarga_lib::YtApp;

#[cfg(not(target_arch = "wasm32"))]
use actix_files::Files;
#[cfg(not(target_arch = "wasm32"))]
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
#[cfg(not(target_arch = "wasm32"))]
use serde::Deserialize;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Deserialize)]
struct DownloadPayload {
    url: String,
    solo_audio: bool,
}

#[cfg(not(target_arch = "wasm32"))]
async fn estado() -> impl Responder {
    HttpResponse::Ok().body("Servidor yt_descarga activo en la LAN")
}

#[cfg(not(target_arch = "wasm32"))]
async fn procesar_descarga_web(payload: web::Json<DownloadPayload>) -> impl Responder {
    let (tx, rx) = std::sync::mpsc::channel::<String>();

    // Hilo dedicado a escuchar y mostrar en la consola del backend lo que yt-dlp reporte
    std::thread::spawn(move || {
        while let Ok(msg) = rx.recv() {
            println!("[yt-dlp] {}", msg);
            if msg == "__FIN__" {
                println!("[BACKEND] ¡Descarga finalizada con éxito!");
                break;
            }
        }
    });

    // Ejecuta la descarga en el backend pasando el canal activo
    yt_descarga_lib::downloader::ejecutar_descarga(payload.url.clone(), payload.solo_audio, tx);

    println!("[BACKEND] Solicitud recibida para: {}", payload.url);

    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "message": "Descarga en proceso"
    }))
}

#[cfg(not(target_arch = "wasm32"))]
fn iniciar_servidor_web() {
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            println!("[+] Servidor Web activo en http://127.0.0.1:8082");
            let _ = HttpServer::new(|| {
                App::new()
                    .route("/api/status", web::get().to(estado))
                    .route("/api/download", web::post().to(procesar_descarga_web))
                    .service(Files::new("/web", "./web"))
                    .service(Files::new("/", ".").index_file("index.html"))
            })
            .bind(("0.0.0.0", 8082)) // Escucha en toda la red local para Android
            .unwrap()
            .run()
            .await;
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    iniciar_servidor_web();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 480.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "YouTube Downloader Portable",
        options,
        Box::new(|cc| Box::new(YtApp::new(cc))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}
