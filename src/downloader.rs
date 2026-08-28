use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::sync::mpsc::Sender;
use std::path::PathBuf;
use crate::runner::{resolver_binarios, YtDlpCommand};

pub fn ejecutar_descarga(url: String, solo_audio: bool, tx: Sender<String>) {
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

        // 2. Ruta de descarga destino (~/Downloads en PC)
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

        // 5. Ejecución con streaming de stdout y stderr
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

        // 6. Notificar a la UI que reactive los botones
        let _ = tx.send("__FIN__".to_string());
    });
}