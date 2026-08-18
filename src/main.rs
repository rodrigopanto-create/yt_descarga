use eframe::egui;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

// ============================================================================
// EMBEBIDO DE BINARIOS
// Incluye directamente los ejecutables en el binario compilado de Rust en
// tiempo de compilación para garantizar que sea 100% portable y autónomo.
// ============================================================================
static YTDLP_BYTES: &[u8] = include_bytes!("../bin/yt-dlp.exe");
static FFMPEG_BYTES: &[u8] = include_bytes!("../bin/ffmpeg.exe");

/// Extrae los binarios embebidos a la carpeta `%TEMP%\yt_descarga_bin` si aún no existen.
///
/// Retorna una tupla con las rutas absolutas: `(yt-dlp.exe, ffmpeg.exe)`
fn preparar_binarios() -> Result<(PathBuf, PathBuf), String> {
    // Definir la ruta del directorio temporal donde residirán los ejecutables
    let temp_dir = env::temp_dir().join("yt_descarga_bin");
    if !temp_dir.exists() {
        fs::create_dir_all(&temp_dir)
            .map_err(|e| format!("Error al crear carpeta temp: {}", e))?;
    }

    let ytdlp_path = temp_dir.join("yt-dlp.exe");
    let ffmpeg_path = temp_dir.join("ffmpeg.exe");

    // Extraer yt-dlp solo si no existe en disco
    if !ytdlp_path.exists() {
        fs::write(&ytdlp_path, YTDLP_BYTES)
            .map_err(|e| format!("Error al extraer yt-dlp.exe: {}", e))?;
    }

    // Extraer ffmpeg solo si no existe en disco
    if !ffmpeg_path.exists() {
        fs::write(&ffmpeg_path, FFMPEG_BYTES)
            .map_err(|e| format!("Error al extraer ffmpeg.exe: {}", e))?;
    }

    Ok((ytdlp_path, ffmpeg_path))
}

// ============================================================================
// PUNTO DE ENTRADA (MAIN)
// Inicializa la ventana nativa mediante egui/eframe.
// ============================================================================
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 480.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "YouTube Downloader Portable (yt-dlp + ffmpeg)",
        options,
        Box::new(|_cc| Box::<YtApp>::default()),
    )
}

// ============================================================================
// ESTRUCTURA PRINCIPAL DEL ESTADO DE LA APLICACIÓN
// ============================================================================
struct YtApp {
    url: String,             // URL capturada en la caja de texto
    logs: String,            // Buffer de texto que acumula la salida del proceso
    esta_descargando: bool,  // Flag para bloquear controles mientras corre una descarga
    rx: Receiver<String>,    // Receptor del canal de comunicación (hilo secundario -> GUI)
    tx: Sender<String>,      // Emisor del canal (para clonar y enviar desde hilos)
}

impl Default for YtApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            url: String::new(),
            logs: String::from("Inicializando binarios portátiles...\n"),
            esta_descargando: false,
            rx,
            tx,
        }
    }
}

impl YtApp {
    /// Inicia el proceso de descarga en un hilo secundario para evitar
    /// bloquear el hilo de renderizado de la interfaz gráfica (GUI).
    fn descargar(&mut self, es_mp3: bool) {
        if self.url.trim().is_empty() {
            self.logs.push_str("[!] Por favor escribe una URL válida.\n");
            return;
        }

        self.esta_descargando = true;
        let formato = if es_mp3 { "MP3" } else { "MP4" };
        self.logs.push_str(&format!("\n=== Iniciando descarga en {} ===\n", formato));

        let url_target = self.url.clone();
        let tx = self.tx.clone();

        // Lanzamiento del hilo secundario de ejecución de tareas
        thread::spawn(move || {
            // 1. Detectar automáticamente la carpeta de Música del usuario activo
            let music_dir = dirs::audio_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."));

            // 2. Extraer o verificar binarios en la carpeta %TEMP%
            let (ytdlp_path, ffmpeg_path) = match preparar_binarios() {
                Ok(paths) => paths,
                Err(err) => {
                    let _ = tx.send(format!("[!] Error fatal: {}\n", err));
                    let _ = tx.send(String::from("__FIN__"));
                    return;
                }
            };

            // 3. Autoactualización previa de yt-dlp (-U) para evitar fallos por cambios en YouTube
            let _ = tx.send(String::from("[!] Verificando actualizaciones de yt-dlp...\n"));
            if let Ok(mut update_cmd) = Command::new(&ytdlp_path)
                .arg("-U")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn() 
            {
                if let Some(stdout) = update_cmd.stdout.take() {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().flatten() {
                        let _ = tx.send(format!("[UPDATE] {}", line));
                    }
                }
                let _ = update_cmd.wait();
            }

            // 4. Configurar el comando principal de yt-dlp
            let mut cmd = Command::new(&ytdlp_path);
            cmd.arg("--ffmpeg-location")
               .arg(&ffmpeg_path)
               .arg("--extractor-args")
               .arg("youtube:player_client=android,web") // Parámetro para prevenir bloqueos por clientes
               .arg("-P")
               .arg(&music_dir);

            // Parámetros específicos según el formato seleccionado
            if es_mp3 {
                cmd.args([
                    "-x",
                    "--audio-format", "mp3",
                    "--audio-quality", "320K",
                    "--embed-metadata",
                    "--embed-thumbnail",
                ]);
            } else {
                cmd.args([
                    "-f", "bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/best",
                    "--merge-output-format", "mp4",
                ]);
            }

            cmd.arg("-o").arg("%(title)s.%(ext)s").arg(&url_target);

            // Redirigir salidas de consola para transmitirlas en tiempo real a la GUI
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());

            if let Ok(mut child) = cmd.spawn() {
                // Capturar salida estándar (STDOUT)
                if let Some(stdout) = child.stdout.take() {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().flatten() {
                        let _ = tx.send(line);
                    }
                }

                // Capturar salida de errores/advertencias (STDERR)
                if let Some(stderr) = child.stderr.take() {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines().flatten() {
                        let _ = tx.send(format!("[LOG] {}", line));
                    }
                }

                let _ = child.wait();
                let _ = tx.send(format!("\n[!] Proceso terminado ({}).\n", formato));
            } else {
                let _ = tx.send(String::from("[!] Error al ejecutar el binario de yt-dlp.\n"));
            }

            // Enviar token de finalización para reactivar la interfaz
            let _ = tx.send(String::from("__FIN__"));
        });
    }
}

// ============================================================================
// DIBUJADO Y LOGICA DE INTERFAZ GRÁFICA (EGUI)
// ============================================================================
impl eframe::App for YtApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drenar mensajes entrantes desde el hilo secundario
        while let Ok(msg) = self.rx.try_recv() {
            if msg == "__FIN__" {
                self.esta_descargando = false;
            } else {
                // Limpiar retornos de carro '\r' para evitar sobreescrituras en la consola
                let limpia = msg.replace('\r', "");
                if !limpia.trim().is_empty() {
                    self.logs.push_str(&limpia);
                    self.logs.push('\n');
                }
            }
        }

        // Si la descarga sigue activa, forzar a egui a repintar el marco en tiempo real
        if self.esta_descargando {
            ctx.request_repaint();
        }

        // Renderizado de componentes UI
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("YouTube Downloader Portable");
            ui.add_space(10.0);

            // Controles superiores: Input + Botones
            ui.horizontal(|ui| {
                ui.label("URL:");
                ui.add_enabled(
                    !self.esta_descargando,
                    egui::TextEdit::singleline(&mut self.url).desired_width(400.0),
                );

                if ui.add_enabled(!self.esta_descargando, egui::Button::new("MP3 (320k)")).clicked() {
                    self.descargar(true);
                }

                if ui.add_enabled(!self.esta_descargando, egui::Button::new("MP4 (Video)")).clicked() {
                    self.descargar(false);
                }
            });

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(5.0);

            ui.label("Consola / Logs:");
            ui.add_space(5.0);

            // Área de texto tipo terminal con scroll horizontal y vertical
            egui::ScrollArea::both()
                .auto_shrink([false; 2])
                .stick_to_bottom(true) // Seguir automáticamente las líneas nuevas
                .show(ui, |scroll_ui| {
                    scroll_ui.add(
                        egui::TextEdit::multiline(&mut self.logs.as_str())
                            .font(egui::TextStyle::Monospace) // Fuente tipo código
                            .desired_width(f32::INFINITY)
                            .interactive(false), // Modo lectura
                    );
                });
        });
    }
}