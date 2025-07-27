mod analyzer;
mod timer;

use std::collections::HashMap;

use eframe::egui;
use wasapi;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([320.0, 240.0])
            .with_drag_and_drop(false),
        ..Default::default()
    };
    let app = MyApp::new();
    eframe::run_native("Vocal Analyzer", options, Box::new(|_cc| Ok(Box::new(app))))
}

struct MyApp {
    timer: Option<timer::Timer>,
    device_names: HashMap<String, String>,
    device_id: String,
}

impl MyApp {
    fn new() -> Self {
        wasapi::initialize_mta().unwrap();
        let direction = wasapi::Direction::Capture;
        let sound_devices = wasapi::DeviceCollection::new(&direction).unwrap();
        let device_names = sound_devices
            .into_iter()
            .map(|device| {
                let device = device.unwrap();
                let device_name = device.get_friendlyname().unwrap();
                let device_id = device.get_id().unwrap();
                (device_id, device_name)
            })
            .collect();
        let default_device = wasapi::get_default_device(&direction).unwrap();
        let device_id = default_device.get_id().unwrap();
        Self {
            timer: None,
            device_names,
            device_id,
        }
    }
    fn is_running(&self) -> bool {
        self.timer.is_some()
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_enabled_ui(!self.is_running(), |ui| {
                egui::ComboBox::from_label("")
                    .selected_text(&self.device_names[&self.device_id])
                    .show_ui(ui, |ui| {
                        for (device_id, device_name) in self.device_names.iter() {
                            let device_id = device_id.to_owned();
                            ui.selectable_value(&mut self.device_id, device_id, device_name);
                        }
                    })
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!self.is_running(), |ui| {
                    if ui.button("Start").clicked() {
                        let mut analyzer = analyzer::Analyzer::new(self.device_id.clone());
                        self.timer = timer::Timer::new(move || analyzer.periodic()).into();
                    }
                });
                if ui.button("Stop").clicked() {
                    self.timer.take();
                }
            });
            ui.add_space(20.0);
            if self.is_running() {
                ui.label(format!(
                    "Status: Runnning on {}...",
                    self.device_names[&self.device_id]
                ));
            } else {
                ui.label("Status: Stopped");
            }
        });
    }
}
