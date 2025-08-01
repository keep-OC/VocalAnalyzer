use std::collections::HashMap;

use crate::analyzer;
use eframe::egui;
use egui_plot::{Bar, BarChart, Line, Plot, PlotPoints};

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
                    let freq_history = analyzer.freq_history_in_midi_note();
                    let history_len = freq_history.len() as f64;
                    let pitch_points: PlotPoints = freq_history
                        .into_iter()
                        .enumerate()
                        .map(|(i, midinote)| [i as f64, midinote as f64])
                        .collect();
                    let pitch = Line::new("pitch", pitch_points)
                        .color(egui::Color32::YELLOW)
                        .width(3.0);
                    let spectrum = analyzer.spectrum();
                    let spec_points: PlotPoints = spectrum
                        .into_iter()
                        .map(|(midinote, gain)| [history_len - gain as f64, midinote as f64])
                        .collect();
                    let spec = Line::new("pitch", spec_points).color(egui::Color32::CYAN);
                    Plot::new("plot")
                        .show_x(false)
                        .y_axis_formatter(|g, _r| midi_note_number_to_str(g.value))
                        .show_axes([false, true])
                        .default_x_bounds(0.0, history_len)
                        .show(ui, |plot_ui| {
                            plot_ui.line(spec);
                            plot_ui.line(pitch);
                        });
                    ctx.request_repaint();
                }
            }
        });
        if self.show_graph && self.is_running() {
            let analyzer = self.analyzer.as_ref().unwrap();
            egui::TopBottomPanel::bottom("bottom")
                .default_height(100.0)
                .resizable(true)
                .show(ctx, |ui| {
                    let gains = analyzer.gains();
                    let gains_bars: Vec<Bar> = gains
                        .into_iter()
                        .enumerate()
                        .map(|(x, y)| Bar::new((1 + x) as f64, y as f64))
                        .collect();
                    let gains_bars = BarChart::new("gains", gains_bars);
                    Plot::new("gains")
                        .default_y_bounds(0.0, 1.0)
                        .show(ui, |plot_ui| {
                            plot_ui.bar_chart(gains_bars);
                        });
                });
            egui::TopBottomPanel::bottom("formant")
                .default_height(100.0)
                .resizable(true)
                .show(ctx, |ui| {
                    let formant_spec = analyzer.formant_spec();
                    let formant_points: PlotPoints = formant_spec
                        .into_iter()
                        .enumerate()
                        .map(|(x, y)| [48_000.0 / 2.0 / 512.0 * x as f64, y])
                        .collect();
                    let line = Line::new("formant", formant_points);
                    let spectrum = analyzer.spectrum();
                    let spec_points: PlotPoints = spectrum
                        .into_iter()
                        .map(|(midinote, gain)| {
                            [
                                440.0 * 2.0_f32.powf((midinote - 69.0) / 12.0) as f64,
                                3.14 * 2.0 + gain as f64 / 2.0,
                            ]
                        })
                        .collect();
                    let spec = Line::new("pitch", spec_points).color(egui::Color32::CYAN);
                    Plot::new("formant").show(ui, |plot_ui| {
                        plot_ui.line(line);
                        plot_ui.line(spec)
                    })
                });
        }
    }
}

fn midi_note_number_to_str(n: f64) -> String {
    if n < 0.0 || 150.0 < n {
        return "".into();
    }
    let notes = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", notes[n as usize % 12], n as isize / 12 - 1)
}
