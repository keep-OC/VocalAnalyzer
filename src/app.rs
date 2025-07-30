use std::collections::HashMap;

use crate::analyzer;
use eframe::egui;
use egui_plot::{Plot, PlotPoints};

pub struct App {
    analyzer: Option<analyzer::Analyzer>,
    device_ids: Vec<String>,
    device_names: HashMap<String, String>,
    device_id: String,
    show_graph: bool,
    spec_bound_max: f32,
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
            spec_bound_max: 0.0,
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
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
                let device_name = &self.device_names[&self.device_id];
                ui.label(format!("Status: Runnning on {}...", device_name));
            } else {
                ui.label("Status: Ready");
            }

            if let Some(analyzer) = &self.analyzer {
                let label = "グラフを表示 (パフォーマンスに影響するかも)";
                ui.checkbox(&mut self.show_graph, label);
                if self.show_graph {
                    let series: PlotPoints = analyzer
                        .freq_history_logscale()
                        .into_iter()
                        .enumerate()
                        .map(|(x, y)| [x as f64, y as f64])
                        .collect();
                    let line = egui_plot::Line::new("pitch", series).color(egui::Color32::YELLOW);
                    egui_plot::Plot::new("plot")
                        .view_aspect(2.0)
                        .sense(egui::Sense::hover())
                        .show_x(false)
                        .show_y(false)
                        .show_axes([false, true])
                        .show(ui, |plot_ui| plot_ui.line(line));
                    let freq_step = 48000.0 / 2.0 / 2048.0;
                    let spectrum = analyzer.spectrum();
                    let power_max = spectrum
                        .iter()
                        .map(|a| a.1)
                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                        .unwrap();
                    const COEFF: f32 = 1.0 / 4.0 / 60.0;
                    self.spec_bound_max =
                        self.spec_bound_max + (power_max - self.spec_bound_max) * COEFF;
                    let bars: Vec<egui_plot::Bar> = spectrum
                        .into_iter()
                        .map(|(x, y)| egui_plot::Bar::new(x as f64, y as f64))
                        .collect();
                    let bar_chart = egui_plot::BarChart::new("pitch", bars)
                        .color(egui::Color32::BLUE)
                        .width(freq_step);
                    Plot::new("spectrum")
                        .view_aspect(2.0)
                        .default_y_bounds(0.0, self.spec_bound_max as f64)
                        .show(ui, |plot_ui| plot_ui.bar_chart(bar_chart));
                    ctx.request_repaint();
                }
            }
        });
    }
}
