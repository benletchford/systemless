use systemless_ui::{eframe, egui, Theme};
struct Hello;
impl eframe::App for Hello {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Hello, Systemless UI");
            ui.label("An independent egui application.");
        });
    }
}
fn main() -> eframe::Result {
    eframe::run_native(
        "Hello",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            Theme::default().apply(&cc.egui_ctx);
            Ok(Box::new(Hello))
        }),
    )
}
