//! Shared look: colors, cards, headings, chips, progress bars.

use crate::model::Modality;
use eframe::egui::{self, Color32, RichText, Ui};

pub const ACCENT: Color32 = Color32::from_rgb(0x3b, 0x82, 0xf6);
pub const GOOD: Color32 = Color32::from_rgb(0x22, 0xc5, 0x5e);
pub const WARN: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);
pub const BAD: Color32 = Color32::from_rgb(0xef, 0x44, 0x44);
pub const HR: Color32 = Color32::from_rgb(0xef, 0x44, 0x44);
pub const PACE: Color32 = Color32::from_rgb(0x3b, 0x82, 0xf6);
pub const SPEED: Color32 = Color32::from_rgb(0x06, 0xb6, 0xd4);
pub const CADENCE: Color32 = Color32::from_rgb(0xa8, 0x55, 0xf7);
pub const ALT: Color32 = Color32::from_rgb(0x84, 0xcc, 0x16);
pub const POWER: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x0b);

/// Zone colors 1..5 (index 0 = below zone 1).
pub const ZONES: [Color32; 6] = [
    Color32::from_rgb(0x94, 0xa3, 0xb8),
    Color32::from_rgb(0x60, 0xa5, 0xfa),
    Color32::from_rgb(0x34, 0xd3, 0x99),
    Color32::from_rgb(0xfa, 0xcc, 0x15),
    Color32::from_rgb(0xfb, 0x92, 0x3c),
    Color32::from_rgb(0xef, 0x44, 0x44),
];

/// Distinct colors for series in comparisons.
pub const SERIES: [Color32; 6] = [
    Color32::from_rgb(0x3b, 0x82, 0xf6),
    Color32::from_rgb(0xf9, 0x73, 0x16),
    Color32::from_rgb(0x10, 0xb9, 0x81),
    Color32::from_rgb(0xe1, 0x1d, 0x48),
    Color32::from_rgb(0x8b, 0x5c, 0xf6),
    Color32::from_rgb(0xea, 0xb3, 0x08),
];

pub fn modality_color(m: Modality) -> Color32 {
    match m {
        Modality::RoadRun => Color32::from_rgb(0x3b, 0x82, 0xf6),
        Modality::Treadmill => Color32::from_rgb(0x06, 0xb6, 0xd4),
        Modality::Tennis => Color32::from_rgb(0x84, 0xcc, 0x16),
        Modality::Strength => Color32::from_rgb(0xa8, 0x55, 0xf7),
        Modality::Other => Color32::from_rgb(0x94, 0xa3, 0xb8),
    }
}

pub fn setup_style(ctx: &egui::Context) {
    // Hack (bundled for monospace) covers arrows and other symbols missing in
    // the proportional font: use it as the last fallback.
    let mut fonts = egui::FontDefinitions::default();
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        list.push("Hack".to_owned());
    }
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.interact_size.y = 24.0;
        use egui::{FontId, TextStyle};
        style.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
        style.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.55);
        // visible outline for text fields and buttons in both themes
        let outline = if style.visuals.dark_mode { Color32::from_gray(70) } else { Color32::from_gray(190) };
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, outline);
        style.visuals.hyperlink_color = ACCENT;
    });
}

pub fn page_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(24.0).strong());
    ui.add_space(6.0);
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).size(16.0).strong());
    ui.add_space(4.0);
}

pub fn weak(ui: &Ui, text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(ui.visuals().weak_text_color())
}

pub fn card_frame(ui: &Ui) -> egui::Frame {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color.gamma_multiply(if ui.visuals().dark_mode { 2.2 } else { 1.0 }))
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(14, 10))
}

/// Indicator card: small title, big value, optional subtitle.
pub fn stat_card(ui: &mut Ui, title: &str, value: &str, sub: Option<&str>, width: f32) -> egui::Response {
    card_frame(ui)
        .show(ui, |ui| {
            ui.set_width(width - 30.0);
            ui.set_min_height(58.0);
            ui.vertical(|ui| {
                ui.label(weak(ui, title).small());
                ui.add(egui::Label::new(RichText::new(value).size(20.0).strong()).truncate());
                if let Some(s) = sub {
                    ui.add(egui::Label::new(weak(ui, s).small()).truncate());
                }
            });
        })
        .response
}

/// Lays out cards in rows that fill the available width.
pub fn card_grid(ui: &mut Ui, min_width: f32, cards: &[(String, String, Option<String>)]) {
    let gap = ui.spacing().item_spacing.x;
    let avail = ui.available_width() - 2.0;
    let per_row = ((avail + gap) / (min_width + gap)).floor().max(1.0) as usize;
    let width = (avail - gap * (per_row as f32 - 1.0)) / per_row as f32;
    for row in cards.chunks(per_row) {
        ui.horizontal(|ui| {
            for (t, v, s) in row {
                stat_card(ui, t, v, s.as_deref(), width);
            }
        });
        ui.add_space(2.0);
    }
}

pub fn chip(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.7)))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().color(color));
        });
}

pub fn modality_chip(ui: &mut Ui, m: Modality) {
    chip(ui, m.label(), modality_color(m));
}

/// Progress bar with "65 / 100 km — 65%".
pub fn progress(ui: &mut Ui, fraction: f32, text: &str, done: bool) {
    let color = if done { GOOD } else { ACCENT };
    ui.add(egui::ProgressBar::new(fraction.clamp(0.0, 1.0)).fill(color).text(text).desired_height(18.0));
}

pub fn empty_state(ui: &mut Ui, text: &str) {
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        ui.label(weak(ui, text).size(15.0));
    });
    ui.add_space(30.0);
}

/// Warning box (comparability notes, estimates, medical disclaimers).
pub fn note(ui: &mut Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.5)))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(if ui.visuals().dark_mode { color.gamma_multiply(1.2) } else { color }));
        });
}

/// Numeric optional input as text (accepts comma). Returns true when changed.
pub fn opt_number_edit(ui: &mut Ui, value: &mut Option<f64>, buf: &mut String, width: f32) -> bool {
    let r = ui.add(egui::TextEdit::singleline(buf).desired_width(width));
    if r.changed() {
        *value = crate::format::parse_num(buf);
        return true;
    }
    false
}

/// Date picker for chrono dates (egui_extras works with jiff dates).
pub fn date_picker(ui: &mut Ui, date: &mut chrono::NaiveDate, id: &str) -> egui::Response {
    use chrono::Datelike;
    let mut j = jiff::civil::Date::new(date.year() as i16, date.month() as i8, date.day() as i8)
        .unwrap_or_else(|_| jiff::civil::Date::new(2026, 1, 1).unwrap());
    let r = ui.add(
        egui_extras::DatePickerButton::new(&mut j)
            .id_salt(id)
            .format(if crate::i18n::pt() { "%d/%m/%Y" } else { "%Y-%m-%d" })
            .calendar_week(false),
    );
    if let Some(d) = chrono::NaiveDate::from_ymd_opt(j.year() as i32, j.month() as u32, j.day() as u32) {
        *date = d;
    }
    r
}

/// Confirmation popup state helper: returns true when "Yes" was clicked.
pub fn confirm_window(ctx: &egui::Context, open: &mut bool, title: &str, text: &str) -> bool {
    let mut yes = false;
    if *open {
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(text);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new(tr!("Sim, confirmar", "Yes, confirm")).color(BAD)).clicked() {
                        yes = true;
                        *open = false;
                    }
                    if ui.button(tr!("Cancelar", "Cancel")).clicked() {
                        *open = false;
                    }
                });
            });
    }
    yes
}
