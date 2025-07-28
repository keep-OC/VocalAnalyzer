use std::collections::HashMap;

use crate::analyzer;
use eframe::egui;

pub struct App {
    analyzer: Option<analyzer::Analyzer>,
    device_ids: Vec<String>,
    device_names: HashMap<String, String>,
    device_id: String,
    show_graph: bool,
}

impl Default for App {
    fn default() -> Self {
        let devices = analyzer::get_devices().unwrap();
        let default_device = analyzer::get_default_device().unwrap();
        Self {
            analyzer: None,
            device_ids: devices
                .iter()
                .map(|device| device.get_id().unwrap())
                .collect(),
            device_names: devices
                .iter()
                .map(|device| {
                    let device_id = device.get_id().unwrap();
                    let device_name = device.get_friendlyname().unwrap();
                    (device_id, device_name)
                })
                .collect(),
            device_id: default_device.get_id().unwrap(),
            show_graph: false,
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        let meiryo = egui::FontData::from_static(include_bytes!("C:/Windows/Fonts/Meiryo.ttc"));
        fonts.font_data.insert("Meiryo".to_owned(), meiryo.into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Meiryo".to_owned());
        cc.egui_ctx.set_fonts(fonts);
        Default::default()
    }

    fn is_running(&self) -> bool {
        self.analyzer.is_some()
    }

    fn start(&mut self) {
        let analyzer = analyzer::Analyzer::new(&self.device_id);
        self.analyzer = analyzer.into();
    }
    fn stop(&mut self) {
        self.analyzer.take();
    }
}

impl eframe::App for App {
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
                        self.start();
                    }
                });
                if ui.button("Stop").clicked() {
                    self.stop();
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
                                .map(|(x, &y)| [x as f64, 69.0 + 12.0 * (y / 440.0).log2() as f64])
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
