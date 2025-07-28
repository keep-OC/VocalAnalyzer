use std::collections::HashMap;

use eframe::egui;
use wasapi;

mod analyzer;
mod osc;

fn main() -> eframe::Result {
    wasapi::initialize_mta().unwrap();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([320.0, 250.0])
            .with_drag_and_drop(false),
        ..Default::default()
    };
    eframe::run_native(
        "Vocal Analyzer",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )
}

struct MyApp {
    analyzer: Option<analyzer::Analyzer>,
    device_ids: Vec<String>,
    device_names: HashMap<String, String>,
    device_id: String,
    show_graph: bool,
}

impl MyApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "Meiryo".to_owned(),
            egui::FontData::from_static(include_bytes!("C:/Windows/Fonts/Meiryo.ttc")).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Meiryo".to_owned());
        cc.egui_ctx.set_fonts(fonts);

        let direction = wasapi::Direction::Capture;
        let devices = analyzer::get_devices().unwrap();
        let device_ids = devices
            .iter()
            .map(|device| device.get_id().unwrap())
            .collect();
        let device_names = devices
            .iter()
            .map(|device| {
                let device_name = device.get_friendlyname().unwrap();
                let device_id = device.get_id().unwrap();
                (device_id, device_name)
            })
            .collect();
        let default_device = wasapi::get_default_device(&direction).unwrap();
        let device_id = default_device.get_id().unwrap();
        Self {
            analyzer: None,
            device_ids,
            device_names,
            device_id,
            show_graph: false,
        }
    }
    fn is_running(&self) -> bool {
        self.analyzer.is_some()
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(!self.is_running(), |ui| {
                egui::ComboBox::from_label("")
                    .selected_text(&self.device_names[&self.device_id])
                    .show_ui(ui, |ui| {
                        for device_id in &self.device_ids {
                            let device_id = device_id.to_owned();
                            let device_name = &self.device_names[&device_id];
                            ui.selectable_value(&mut self.device_id, device_id, device_name);
                        }
                    })
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!self.is_running(), |ui| {
                    if ui.button("Start").clicked() {
                        let analyzer = analyzer::Analyzer::new(&self.device_id);
                        self.analyzer = analyzer.into();
                    }
                });
                if ui.button("Stop").clicked() {
                    self.analyzer.take();
                }
            });
            if self.is_running() {
                ui.label(format!(
                    "Status: Runnning on {}...",
                    self.device_names[&self.device_id]
                ));
            } else {
                ui.label("Status: Ready");
            }

            if let Some(analyzer) = &self.analyzer {
                ui.checkbox(
                    &mut self.show_graph,
                    "グラフを表示 (パフォーマンスに影響するかも)",
                );
                if self.show_graph {
                    egui_plot::Plot::new("plot")
                        .view_aspect(2.0)
                        .sense(egui::Sense::hover())
                        .show_x(false)
                        .show_y(false)
                        .show_axes([false, true])
                        .show(ui, |plot_ui| {
                            let lock = analyzer.detected_piches.lock().unwrap();
                            let series: egui_plot::PlotPoints = lock
                                .iter()
                                .enumerate()
                                .map(|(x, &y)| [x as f64, y.log(10.0) as f64])
                                .collect();
                            let line = egui_plot::Line::new("pitch", series);
                            plot_ui.line(line);
                        });
                    ctx.request_repaint();
                }
            }
        });
    }
}
