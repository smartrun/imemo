//! iMemo Reader for Windows - Entry point
//! Non-standard Matrix Code (IMC) Screen Capture & Document Restorer

use eframe::egui::Vec2;
use imemo_reader_win::app::ImemoApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("iMemo 리더기 (Windows)")
            .with_inner_size(Vec2::new(880.0, 680.0))
            .with_min_inner_size(Vec2::new(760.0, 540.0)),
        ..Default::default()
    };

    eframe::run_native(
        "iMemo 리더기",
        native_options,
        Box::new(|cc| Ok(Box::new(ImemoApp::new(cc)))),
    )
}
