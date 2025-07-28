mod analyzer;
mod app;
mod osc;

fn main() -> eframe::Result {
    wasapi::initialize_mta().unwrap();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([320.0, 260.0])
            .with_drag_and_drop(false),
        ..Default::default()
    };
    eframe::run_native(
        "Vocal Analyzer",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
