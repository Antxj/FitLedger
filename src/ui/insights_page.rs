use super::App;
use eframe::egui::Ui;

#[derive(Default)]
pub struct State;

pub fn show(_app: &mut App, ui: &mut Ui) {
    super::widgets::empty_state(ui, "…");
}
