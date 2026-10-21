// Módulo extractor para utilidades de parseo/extracción
pub fn extraer_id_video(url: &str) -> Option<String> {
    if url.contains("v=") {
        url.split("v=").nth(1).map(|s| s.split('&').next().unwrap_or(s).to_string())
    } else if url.contains("youtu.be/") {
        url.split("youtu.be/").nth(1).map(|s| s.split('?').next().unwrap_or(s).to_string())
    } else {
        None
    }
}