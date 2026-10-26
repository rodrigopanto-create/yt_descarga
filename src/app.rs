use crate::android_yt_dlp::content_rect_top_px;
use crate::downloader::ejecutar_descarga;
use crate::platform::obtener_portapapeles_jni;
use eframe::egui;
use std::sync::mpsc::{channel, Receiver, Sender};

pub struct YtApp {
    url: String,
    logs: String,
    esta_descargando: bool,
    tx: Sender<String>,
    rx: Receiver<String>,
}

impl Default for YtApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            url: String::new(),
            logs: "Aplicación iniciada...\n".to_string(),
            esta_descargando: false,
            tx,
            rx,
        }
    }
}

impl YtApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    fn descargar(&mut self, es_mp3: bool) {
        if self.url.trim().is_empty() {
            self.logs.push_str("[!] Error: Ingresa una URL válida.\n");
            return;
        }

        let tipo = if es_mp3 { "MP3" } else { "MP4" };
        self.logs
            .push_str(&format!("[+] Solicitando archivo {}...\n", tipo));
        self.esta_descargando = true;

        let url = self.url.trim().to_string();
        let tx = self.tx.clone();

        ejecutar_descarga(url, es_mp3, tx);
    }
}

impl eframe::App for YtApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Recibir mensajes de progreso
        while let Ok(msg) = self.rx.try_recv() {
            if msg == "__FIN__" {
                self.esta_descargando = false; // Reactiva los botones
            } else {
                self.logs.push_str(&format!("{}\n", msg));
            }
        }

        if self.esta_descargando {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let top_px = content_rect_top_px() as f32;
            let top_pts = top_px / ctx.pixels_per_point();
            let inset = top_pts.max(28.0);
            ui.add_space(inset + 8.0);

            ui.heading("YouTube Downloader Portable");
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("URL:");
                ui.text_edit_singleline(&mut self.url);
            });

            ui.add_space(5.0);

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.esta_descargando, egui::Button::new("📋 PEGAR"))
                    .clicked()
                {
                    if let Some(txt) = obtener_portapapeles_jni() {
                        if !txt.is_empty() {
                            self.url = txt;
                            self.logs
                                .push_str("[✓] Enlace pegado desde el portapapeles.\n");
                        } else {
                            self.logs.push_str("[!] El portapapeles está vacío.\n");
                        }
                    } else {
                        self.logs
                            .push_str("[!] No se pudo leer el portapapeles vía JNI.\n");
                    }
                }

                if ui
                    .add_enabled(!self.esta_descargando, egui::Button::new("🎵 MP3"))
                    .clicked()
                {
                    self.descargar(true);
                }

                if ui
                    .add_enabled(!self.esta_descargando, egui::Button::new("🎬 MP4"))
                    .clicked()
                {
                    self.descargar(false);
                }
            });

            ui.add_space(15.0);
            ui.separator();
            ui.label("Consola / Logs:");

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.logs) // Pasado directamente como &mut String
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(15)
                        .lock_focus(true),
                );
            });
        });
    }
}
