use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Vocal Analyzer",
        options,
        Box::new(|_cc| Ok(Box::<MyApp>::default())),
    )
}

#[derive(Default)]
struct MyApp {
    is_running: bool,
    sound_device_index: i32,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ComboBox::from_label("")
                .selected_text(format!("{:?}", self.sound_device_index))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sound_device_index, 0, "one");
                    ui.selectable_value(&mut self.sound_device_index, 1, "two");
                    ui.selectable_value(&mut self.sound_device_index, 2, "three");
                });
            ui.horizontal(|ui| {
                if ui.button("Start").clicked() {
                    self.is_running = true;
                }
                if ui.button("Stop").clicked() {
                    self.is_running = false;
                }
            });
            ui.add_space(20.0);
            if self.is_running {
                ui.label("Status: Runnning...");
            } else {
                ui.label("Status: Stopped");
            }
        });
    }
}
