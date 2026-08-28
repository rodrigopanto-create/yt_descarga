use eframe::egui;
use yt_descarga_lib::YtApp;

fn main() -> eframe::Result<()> {
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