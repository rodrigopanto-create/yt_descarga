use std::sync::mpsc::Sender;

// -------------------------------------------------------------
// IMPLEMENTACIÓN PARA NATIVO (WINDOWS / LINUX / ANDROID)
// -------------------------------------------------------------
#[cfg(not(target_arch = "wasm32"))]
pub fn ejecutar_descarga(url: String, solo_audio: bool, tx: Sender<String>) {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::path::PathBuf;
    use crate::runner::{resolver_binarios, YtDlpCommand};

    std::thread::spawn(move || {
        let mandar_log = |msg: &str| {
            let _ = tx.send(msg.to_string());
        };

        mandar_log("[+] Iniciando estrategia de búsqueda de binarios...");

        // 1. Resolver binarios
        let binarios = match resolver_binarios() {
            Ok(paths) => paths,
            Err(e) => {
                mandar_log(&format!("[!] Error fatal: {}", e));
                let _ = tx.send("__FIN__".to_string());
                return;
            }
        };

        // 2. Ruta de descarga destino
        let output_dir = if cfg!(target_os = "android") {
            PathBuf::from("/sdcard/Download")
        } else {
            dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
        };
        let output_str = output_dir.to_string_lossy().to_string();

        // 3. Auto-actualización oficial
        mandar_log("[+] Verificando actualizaciones de yt-dlp (-U)...");
        let mut update_cmd = std::process::Command::new(&binarios.ytdlp);
        update_cmd.arg("-U");
        if let Ok(out) = update_cmd.output() {
            let log_up = String::from_utf8_lossy(&out.stdout);
            mandar_log(&format!("[yt-dlp -U]: {}", log_up.trim()));
        }

        // 4. Preparar comando
        let runner = YtDlpCommand::new(binarios.ytdlp, binarios.ffmpeg);
        let mut cmd = if solo_audio {
            runner.descargar_audio(&url, &output_str)
        } else {
            runner.descargar_video(&url, &output_str)
        };

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        mandar_log("[+] Iniciando motor de descarga...");

        // 5. Ejecución con streaming de logs
        match cmd.spawn() {
            Ok(mut child) => {
                if let Some(stdout) = child.stdout.take() {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().flatten() {
                        mandar_log(&line);
                    }
                }

                if let Some(stderr) = child.stderr.take() {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines().flatten() {
                        mandar_log(&format!("[ERR]: {}", line));
                    }
                }

                match child.wait() {
                    Ok(status) if status.success() => {
                        mandar_log("--------------------------------------------------");
                        mandar_log(&format!("¡ÉXITO! El archivo ya está listo en: {}", output_str));
                    }
                    _ => mandar_log("[!] El proceso de descarga finalizó con errores."),
                }
            }
            Err(e) => mandar_log(&format!("[!] Error al ejecutar proceso: {}", e)),
        }

        // 6. Notificar reactivación a la UI
        let _ = tx.send("__FIN__".to_string());
    });
}

// -------------------------------------------------------------
// IMPLEMENTACIÓN PARA WEBASSEMBLY (NAVEGADOR)
// -------------------------------------------------------------
#[cfg(target_arch = "wasm32")]
pub fn ejecutar_descarga(url: String, solo_audio: bool, tx: Sender<String>) {
    let mandar_log = |msg: &str| {
        let _ = tx.send(msg.to_string());
    };

    mandar_log("[+] Enviando solicitud al servidor Actix...");

    wasm_bindgen_futures::spawn_local(async move {
        // Obtener la URL base actual del navegador (ej: http://127.0.0.1:8080 o http://192.168.1.15:8080)
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());

        let api_url = format!("{}/api/download", origin);

        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "url": url,
            "solo_audio": solo_audio
        });

        match client.post(&api_url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                let _ = tx.send("[✓] Servidor inició la descarga en el backend.".to_string());
            }
            Ok(resp) => {
                let _ = tx.send(format!("[!] Error del servidor: HTTP {}", resp.status()));
            }
            Err(e) => {
                let _ = tx.send(format!("[!] Error de red: {}", e));
            }
        }

        let _ = tx.send("__FIN__".to_string());
    });
}