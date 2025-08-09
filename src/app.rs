use std::io::Read;

use crate::{
    analyzer::{Analyzer, CHUNK_SIZE},
    sound_device::DeviceList,
    utils,
};
use eframe::egui;
use egui_plot::{Bar, BarChart, Line, Plot, PlotPoints};

pub struct App {
    device_list: DeviceList,
    analyzer: Option<Analyzer>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            device_list: DeviceList::new(),
            analyzer: None,
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        let mut fontfile = std::fs::File::open("C:/Windows/Fonts/Meiryo.ttc").unwrap();
        let mut fontdata = Vec::new();
        fontfile.read_to_end(&mut fontdata).unwrap();
        let meiryo = egui::FontData::from_owned(fontdata);
        fonts.font_data.insert("Meiryo".to_owned(), meiryo.into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Meiryo".to_owned());
        cc.egui_ctx.set_fonts(fonts);
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        Default::default()
    }

    fn is_running(&self) -> bool {
        self.analyzer.is_some()
    }

    fn start(&mut self) {
        let capturer = self.device_list.device().capturer(CHUNK_SIZE);
        let analyzer = Analyzer::new(capturer);
        self.analyzer = analyzer.into();
    }

    fn stop(&mut self) {
        self.analyzer.take();
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top")
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!self.is_running(), |ui| {
                        let combobox = egui::ComboBox::from_label("")
                            .selected_text(&self.device_list.device().name);
                        combobox.show_ui(ui, |ui| {
                            for (i, device) in self.device_list.devices.iter().enumerate() {
                                ui.selectable_value(&mut self.device_list.index, i, &device.name);
                            }
                        });
                        if ui.button("Start").clicked() {
                            self.start();
                        }
                    });
                    if ui.button("Stop").clicked() {
                        self.stop();
                    }
                });
            });
        let is_focused = ctx.input(|i| i.focused);
        if is_focused && self.is_running() {
            let analyzer = self.analyzer.as_ref().unwrap();
            update_bottom(analyzer, ctx);
            update_main(analyzer, ctx);
            ctx.request_repaint();
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                if self.is_running() {
                    ui.heading("実行中...");
                    ui.label("リソースの節約のためにグラフを非表示にしています。");
                }
            });
        }
    }
}

fn update_main(analyzer: &Analyzer, ctx: &egui::Context) {
    let freq_history = analyzer.results.freq_history_in_midi_note();
    let history_len = freq_history.len() as f64;
    let pitch_points: PlotPoints = freq_history
        .into_iter()
        .enumerate()
        .map(|(i, midinote)| [i as f64, midinote as f64])
        .collect();
    let pitch = Line::new("pitch", pitch_points)
        .color(egui::Color32::YELLOW)
        .width(3.0);

    let spectrum = analyzer.results.spectrum();
    let spec_points: PlotPoints = spectrum
        .into_iter()
        .map(|(midinote, gain)| {
            let gain = gain as f64 * 2.0 + 3.0;
            [history_len - 1.0 - gain, midinote as f64]
        })
        .collect();
    let spec = Line::new("pitch", spec_points).color(egui::Color32::CYAN);

    let gain = utils::normalize(analyzer.results.volume_db(), -40.0, 0.0).clamp(0.0, 1.0);
    let progress_bar: egui::ProgressBar = egui::ProgressBar::new(gain)
        .desired_height(10.0)
        .corner_radius(1);

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add(progress_bar);
        ui.add_space(10.0);
        Plot::new("plot")
            .show_x(false)
            .y_axis_formatter(|g, _r| midi_note_number_to_str(g.value))
            .show_axes([false, true])
            .default_x_bounds(0.0, history_len)
            .show(ui, |plot_ui| {
                plot_ui.line(spec);
                plot_ui.line(pitch);
            });
    });
}

fn update_bottom(analyzer: &Analyzer, ctx: &egui::Context) {
    let gains = analyzer.results.gains();
    let gains_bars: Vec<Bar> = gains
        .into_iter()
        .enumerate()
        .map(|(x, y)| Bar::new((1 + x) as f64, y as f64))
        .collect();
    let gains_bars = BarChart::new("gains", gains_bars);

    egui::TopBottomPanel::bottom("bottom")
        .default_height(80.0)
        .resizable(true)
        .show(ctx, |ui| {
            Plot::new("gains")
                .default_y_bounds(0.0, 1.0)
                .show(ui, |plot_ui| {
                    plot_ui.bar_chart(gains_bars);
                });
        });

    let (min, max) = (-30.0, 20.0);
    let formant_spec = analyzer.results.formant_spec();
    let formant_points: PlotPoints = formant_spec
        .into_iter()
        .enumerate()
        .map(|(x, y)| [48_000.0 / 4.0 / 512.0 * x as f64, y.clamp(min, max)])
        .collect();
    let formantspec_line = Line::new("formant", formant_points);

    let spectrum = analyzer.results.spectrum();
    let spec_points: PlotPoints = spectrum
        .into_iter()
        .map(|(midinote, gain)| {
            let freq = 440.0 * 2.0_f32.powf((midinote - 69.0) / 12.0) as f64;
            let gain = (gain as f64 + 6.02).clamp(min, max);
            [freq, gain]
        })
        .collect();
    let spec = Line::new("pitch", spec_points).color(egui::Color32::CYAN);

    let peaks = analyzer.results.formant_peak();
    let colors = [
        egui::Color32::RED,
        egui::Color32::GREEN,
        egui::Color32::BLUE,
        egui::Color32::MAGENTA,
    ];

    egui::TopBottomPanel::bottom("formant")
        .default_height(100.0)
        .resizable(true)
        .show(ctx, |ui| {
            Plot::new("formant").show(ui, |plot_ui| {
                plot_ui.line(formantspec_line);
                plot_ui.line(spec);
                peaks.iter().take(4).zip(colors).for_each(|(&f, c)| {
                    let points: PlotPoints = vec![[f, min], [f, max]].into();
                    let line = Line::new("peak", points).color(c);
                    plot_ui.line(line);
                });
            });
        });
}

fn midi_note_number_to_str(n: f64) -> String {
    if !(0.0..=150.0).contains(&n) {
        return "".into();
    }
    let notes = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", notes[n as usize % 12], n as isize / 12 - 1)
}
