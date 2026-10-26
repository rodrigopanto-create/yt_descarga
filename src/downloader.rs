use std::io::{BufRead, BufReader, Read};
use std::sync::mpsc::Sender;

// Ejecución Android en hilo secundario para mantener fluida la interfaz.
pub fn ejecutar_descarga(url: String, solo_audio: bool, tx: Sender<String>) {
    use crate::runner::{resolver_binarios, YtDlpCommand};
    use std::fs;
    use std::process::Stdio;
    use std::sync::atomic::{AtomicU64, Ordering};

    static DOWNLOAD_ID: AtomicU64 = AtomicU64::new(0);

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

        if solo_audio && binarios.ffmpeg.is_none() {
            mandar_log("[!] La conversión a MP3 requiere FFmpeg y FFprobe para Android. No se inició la descarga.");
            let _ = tx.send("__FIN__".to_string());
            return;
        }

        // Cada proceso escribe en una carpeta privada y aislada antes de publicar el resultado.
        let staging_root = match crate::android_yt_dlp::download_staging_dir() {
            Ok(path) => path,
            Err(error) => {
                mandar_log(&format!(
                    "[!] No se pudo preparar el almacenamiento: {error}"
                ));
                let _ = tx.send("__FIN__".to_string());
                return;
            }
        };
        let timestamp = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_micros(),
            Err(error) => {
                mandar_log(&format!(
                    "[!] No se pudo generar un identificador de descarga: {error}"
                ));
                let _ = tx.send("__FIN__".to_string());
                return;
            }
        };
        let output_dir = staging_root.join(format!(
            "download-{timestamp}-{}",
            DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed)
        ));
        if let Err(error) = fs::create_dir(&output_dir) {
            mandar_log(&format!(
                "[!] No se pudo crear el directorio temporal {}: {error}",
                output_dir.display()
            ));
            let _ = tx.send("__FIN__".to_string());
            return;
        }
        let output_str = output_dir.to_string_lossy().to_string();

        let runner = YtDlpCommand::new(binarios.ytdlp, binarios.ffmpeg);
        let mut cmd = if solo_audio {
            runner.descargar_audio(&url, &output_str)
        } else {
            runner.descargar_video(&url, &output_str)
        };

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        mandar_log(&format!("[DEBUG] Comando a ejecutar: {:?}", cmd));

        match cmd.spawn() {
            Ok(mut child) => {
                let stdout_reader = child.stdout.take().map(|stdout| {
                    let tx = tx.clone();
                    std::thread::spawn(move || forward_output(stdout, "", tx))
                });
                let stderr_reader = child.stderr.take().map(|stderr| {
                    let tx = tx.clone();
                    std::thread::spawn(move || forward_output(stderr, "[ERR]: ", tx))
                });

                let status = child.wait();
                for reader in [stdout_reader, stderr_reader].into_iter().flatten() {
                    if reader.join().is_err() {
                        mandar_log("[!] Falló el hilo que leía la salida de yt-dlp.");
                    }
                }

                match status {
                    Ok(status) if status.success() => {
                        match publicar_archivos(&output_dir, &mandar_log) {
                            Ok(()) => {
                                mandar_log("--------------------------------------------------");
                                mandar_log("[✓] Descarga publicada correctamente.");
                                if let Err(error) = fs::remove_dir_all(&output_dir) {
                                    mandar_log(&format!(
                                        "[!] No se pudo limpiar el directorio temporal {}: {error}",
                                        output_dir.display()
                                    ));
                                }
                            }
                            Err(error) => mandar_log(&format!(
                                "[!] La descarga terminó, pero no se pudo guardar en Descargas: {error}. El archivo temporal se conserva en {}",
                                output_dir.display()
                            )),
                        }
                    }
                    Ok(status) => mandar_log(&format!(
                        "[!] yt-dlp terminó con estado {}. Los archivos temporales se conservan en {}",
                        status,
                        output_dir.display()
                    )),
                    Err(error) => mandar_log(&format!(
                        "[!] No se pudo esperar a yt-dlp: {error}. Los archivos temporales se conservan en {}",
                        output_dir.display()
                    )),
                }
            }
            Err(error) => {
                mandar_log(&format!("[!] Error al ejecutar proceso: {error}"));
                if let Err(cleanup_error) = fs::remove_dir_all(&output_dir) {
                    mandar_log(&format!(
                        "[!] No se pudo limpiar el directorio temporal {}: {cleanup_error}",
                        output_dir.display()
                    ));
                }
            }
        }

        let _ = tx.send("__FIN__".to_string());
    });
}

fn forward_output<R>(reader: R, prefix: &'static str, tx: Sender<String>)
where
    R: Read + Send + 'static,
{
    for line in BufReader::new(reader).lines() {
        match line {
            Ok(line) => {
                let _ = tx.send(format!("{prefix}{line}"));
            }
            Err(error) => {
                let _ = tx.send(format!("[!] Error leyendo la salida de yt-dlp: {error}"));
                break;
            }
        }
    }
}

fn publicar_archivos(output_dir: &std::path::Path, log: &impl Fn(&str)) -> Result<(), String> {
    use std::fs;

    let mut files = fs::read_dir(output_dir)
        .map_err(|error| format!("No se pudo inspeccionar {}: {error}", output_dir.display()))?
        .map(|entry| {
            entry
                .map_err(|error| format!("No se pudo leer un resultado de yt-dlp: {error}"))
                .and_then(|entry| {
                    let file_type = entry.file_type().map_err(|error| {
                        format!("No se pudo inspeccionar el resultado: {error}")
                    })?;
                    Ok((entry.path(), file_type.is_file()))
                })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(path, is_file)| is_file.then_some(path))
        .collect::<Vec<_>>();
    files.sort();
    if files.is_empty() {
        return Err("yt-dlp terminó sin producir un archivo de salida".to_string());
    }

    for file in files {
        match crate::platform::publicar_descarga(&file) {
            Ok(location) => log(&format!("[✓] Guardado: {location}")),
            Err(error) => {
                return Err(format!(
                    "{}: {error}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }
    Ok(())
}
