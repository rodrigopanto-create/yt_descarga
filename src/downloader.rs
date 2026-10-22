use std::sync::mpsc::Sender;

// -------------------------------------------------------------
// IMPLEMENTACIÓN PARA NATIVO (WINDOWS / LINUX / ANDROID)
// -------------------------------------------------------------
#[cfg(not(target_arch = "wasm32"))]
pub fn ejecutar_descarga(url: String, solo_audio: bool, tx: Sender<String>) {
    use crate::runner::{resolver_binarios, YtDlpCommand};
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    std::thread::spawn(move || {
        let mandar_log = |msg: &str| {
            let _ = tx.send(msg.to_string());
        };

        mandar_log("[+] Localizando binarios y entorno de ejecución...");

        // 1. Resolver binarios (runner.rs se encarga de ubicar el entorno según la plataforma)
        let binarios = match resolver_binarios() {
            Ok(paths) => paths,
            Err(e) => {
                mandar_log(&format!("[!] Error fatal al cargar binarios: {}", e));
                let _ = tx.send("__FIN__".to_string());
                return;
            }
        };

        // 2. Ruta de descarga destino
        #[cfg(target_os = "android")]
        let output_dir = match crate::android_downloads::prepare_download() {
            Ok(path) => path,
            Err(error) => {
                mandar_log(&format!("[!] No se pudo preparar Descargas: {error}"));
                let _ = tx.send("__FIN__".to_string());
                return;
            }
        };
        #[cfg(not(target_os = "android"))]
        let output_dir = dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let output_str = output_dir.to_string_lossy().to_string();

        // 3. Preparar comando (Soporte nativo para el python3_pie en Android o runner clásico en Escritorio)
        let runner = YtDlpCommand::new(binarios.ytdlp, binarios.ffmpeg);
        let mut cmd = if solo_audio {
            runner.descargar_audio(&url, &output_str)
        } else {
            runner.descargar_video(&url, &output_str)
        };

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        mandar_log(&format!("[DEBUG] Comando a ejecutar: {:?}", cmd));

        // 4. Ejecución con streaming de logs en tiempo real a la UI
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
                        #[cfg(target_os = "android")]
                        match crate::android_downloads::publish_download(&output_dir) {
                            Ok(uris) => {
                                mandar_log("¡ÉXITO! Guardado en Descargas/yt_descarga:");
                                for uri in uris {
                                    mandar_log(&uri);
                                }
                            }
                            Err(error) => {
                                mandar_log(&format!("[!] No se pudo publicar la descarga: {error}"))
                            }
                        }
                        #[cfg(not(target_os = "android"))]
                        {
                            mandar_log("--------------------------------------------------");
                            mandar_log(&format!(
                                "¡ÉXITO! El archivo está guardado en: {}",
                                output_str
                            ));
                        }
                    }
                    _ => mandar_log("[!] El proceso finalizó con errores."),
                }
            }
            Err(e) => mandar_log(&format!("[!] Error al ejecutar proceso: {}", e)),
        }

        #[cfg(target_os = "android")]
        if output_dir.exists() {
            mandar_log(&format!(
                "[!] Staging conservado para diagnóstico o recuperación: {}",
                output_dir.display()
            ));
        }
        // 5. Notificar reactivación a la UI
        let _ = tx.send("__FIN__".to_string());
    });
}

// -------------------------------------------------------------
// IMPLEMENTACIÓN PARA WEBASSEMBLY (NAVEGADOR)
// -------------------------------------------------------------
#[cfg(target_arch = "wasm32")]
pub fn ejecutar_descarga(url: String, solo_audio: bool, tx: Sender<String>) {
    if web_sys::window()
        .and_then(|window| window.location().hostname().ok())
        .is_some_and(|hostname| hostname.ends_with(".github.io"))
    {
        let _ = tx.send(
            "[!] GitHub Pages solo aloja la interfaz. Para descargar, abre el servidor Rust local en http://127.0.0.1:8082."
                .to_string(),
        );
        let _ = tx.send("__FIN__".to_string());
        return;
    }

    let mandar_log = |msg: &str| {
        let _ = tx.send(msg.to_string());
    };

    mandar_log("[+] Enviando solicitud al servidor Actix...");

    wasm_bindgen_futures::spawn_local(async move {
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_else(|| "http://127.0.0.1:8082".to_string());

        let api_url = format!("{}/api/download", origin);
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "url": url,
            "solo_audio": solo_audio
        });

        match client.post(&api_url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                let _ = tx.send("[✓] ¡Descarga completada con éxito en la PC!".to_string());
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
