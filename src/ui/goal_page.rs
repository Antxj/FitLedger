//! The goal: distance, optional finish time, optional heart-rate cap, days
//! per week. Saving a change rebuilds the plan from today.

use super::widgets::{self, weak};
use super::{App, Page};
use crate::coach::{Goal, Surface};
use crate::format;
use eframe::egui::{self, RichText, Ui};

/// Longest goal distance. The coach's rules (long run up to the goal
/// distance, one taper week) are meant for distances up to 10 km.
pub const MAX_KM: f64 = 10.0;

/// Text buffers of the goal form.
pub struct Form {
    pub g: Goal,
    distance: String,
    time: String,
    hr: String,
    pace: String,
    use_date: bool,
    date: chrono::NaiveDate,
    pub error: Option<String>,
}

impl Form {
    pub fn new(g: Goal) -> Form {
        let today = chrono::Local::now().date_naive();
        Form {
            distance: format::num_trim(g.distance_m / 1000.0, 2),
            time: g.target_time_s.map(format::duration).unwrap_or_default(),
            hr: g.target_hr.map(|h| format!("{}", h.round())).unwrap_or_default(),
            pace: g.hr_pace.map(format::pace).unwrap_or_default(),
            use_date: g.target_date.is_some(),
            date: g.target_date.unwrap_or(today + chrono::Duration::days(84)),
            error: None,
            g,
        }
    }

    /// Validates the buffers into the goal.
    fn apply(&mut self) -> bool {
        self.error = None;
        let Some(km) = format::parse_num(&self.distance).filter(|v| (0.4..=MAX_KM).contains(v)) else {
            self.error = Some(
                tr!(
                    "Informe a distância, de 0,4 a 10 km. O treinador foi pensado para metas de até 10 km.",
                    "Enter the distance, from 0.4 to 10 km. The coach is designed for goals up to 10 km."
                )
                .into(),
            );
            return false;
        };
        self.g.distance_m = km * 1000.0;
        self.g.target_time_s = if self.time.trim().is_empty() {
            None
        } else {
            match format::parse_duration(&self.time).filter(|t| *t > 60.0) {
                Some(t) => Some(t),
                None => {
                    self.error = Some(
                        tr!(
                            "Tempo inválido. Use mm:ss ou h:mm:ss.",
                            "Invalid time. Use mm:ss or h:mm:ss."
                        )
                        .into(),
                    );
                    return false;
                }
            }
        };
        self.g.target_hr = if self.hr.trim().is_empty() {
            None
        } else {
            match format::parse_num(&self.hr).filter(|h| (90.0..220.0).contains(h)) {
                Some(h) => Some(h),
                None => {
                    self.error = Some(
                        tr!(
                            "FC inválida (entre 90 e 220 bpm).",
                            "Invalid HR (between 90 and 220 bpm)."
                        )
                        .into(),
                    );
                    return false;
                }
            }
        };
        self.g.hr_pace = if self.g.target_hr.is_some() && self.g.target_time_s.is_none() {
            format::parse_pace(&self.pace)
        } else {
            None
        };
        self.g.target_date = self.use_date.then_some(self.date);
        true
    }
}

/// The form itself. Returns the goal when "save" is clicked and it is valid.
pub fn form(ui: &mut Ui, f: &mut Form, save_label: &str) -> Option<Goal> {
    let mut saved = None;
    egui::Grid::new("goal-form")
        .num_columns(2)
        .spacing([14.0, 10.0])
        .show(ui, |ui| {
            ui.label(RichText::new(tr!("Distância", "Distance")).strong());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut f.distance).desired_width(70.0));
                ui.label("km");
                for km in ["3", "5", "8", "10"] {
                    if ui.small_button(format!("{km} km")).clicked() {
                        f.distance = km.into();
                    }
                }
            });
            ui.end_row();
            ui.label(RichText::new(tr!("Tempo (opcional)", "Time (optional)")).strong());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut f.time).desired_width(70.0));
                ui.label("mm:ss");
                if let (Some(t), Some(km)) = (format::parse_duration(&f.time), format::parse_num(&f.distance))
                    && t > 60.0
                    && km > 0.0
                {
                    let p = t / km;
                    ui.label(weak(
                        ui,
                        trf!(
                            "= {} · {} km/h",
                            "= {} · {} km/h",
                            format::pace_unit(p),
                            format::num(format::pace_to_kmh(p), 1)
                        ),
                    ));
                }
            });
            ui.end_row();
            ui.label(RichText::new(tr!("FC média máxima (opcional)", "Max average HR (optional)")).strong());
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut f.hr).desired_width(70.0));
                ui.label("bpm");
                ui.label(
                    weak(
                        ui,
                        tr!(
                            "ex.: correr a distância com FC média abaixo disso",
                            "e.g. run the distance with an average HR below this"
                        ),
                    )
                    .small(),
                );
            });
            ui.end_row();
            let hr_goal = !f.hr.trim().is_empty() && f.time.trim().is_empty();
            if hr_goal {
                ui.label(RichText::new(tr!("No pace", "At pace")).strong());
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut f.pace)
                            .desired_width(70.0)
                            .hint_text("6:30"),
                    );
                    ui.label("/km");
                    match format::parse_pace(&f.pace) {
                        Some(p) => ui.label(weak(ui, format!("= {} km/h", format::num(format::pace_to_kmh(p), 1)))),
                        None => ui.label(
                            weak(
                                ui,
                                tr!("em branco: no seu ritmo leve atual", "blank: at your current easy pace"),
                            )
                            .small(),
                        ),
                    };
                });
                ui.end_row();
            }
            ui.label(RichText::new(tr!("Dias por semana", "Days per week")).strong());
            ui.horizontal(|ui| {
                for n in 2..=6u8 {
                    ui.selectable_value(&mut f.g.days_per_week, n, n.to_string());
                }
            });
            ui.end_row();
            ui.label(RichText::new(tr!("Quais dias (opcional)", "Which days (optional)")).strong());
            ui.horizontal(|ui| {
                let monday = crate::coach::week_start(chrono::Local::now().date_naive());
                for d in 0..7 {
                    let name = format::weekday_short(monday + chrono::Duration::days(d as i64));
                    ui.toggle_value(&mut f.g.preferred_days[d], name);
                }
                let n = f.g.preferred_days.iter().filter(|x| **x).count();
                if n > 0 && n != f.g.days_per_week as usize {
                    ui.label(
                        RichText::new(trf!(
                            "marque {} dias (ou nenhum)",
                            "pick {} days (or none)",
                            f.g.days_per_week
                        ))
                        .small()
                        .color(widgets::WARN),
                    );
                }
            });
            ui.end_row();
            ui.label(RichText::new(tr!("Dia do longão", "Long run day")).strong());
            ui.horizontal(|ui| {
                let monday = crate::coach::week_start(chrono::Local::now().date_naive());
                for d in 0..7u8 {
                    ui.selectable_value(
                        &mut f.g.long_run_day,
                        d,
                        format::weekday_short(monday + chrono::Duration::days(d as i64)),
                    );
                }
            });
            ui.end_row();
            ui.label(RichText::new(tr!("Onde vai correr", "Where you run")).strong());
            ui.horizontal(|ui| {
                for s in [Surface::Any, Surface::Treadmill, Surface::Road] {
                    ui.selectable_value(&mut f.g.surface, s, s.label());
                }
            });
            ui.end_row();
            ui.label(RichText::new(tr!("Data alvo (opcional)", "Target date (optional)")).strong());
            ui.horizontal(|ui| {
                ui.checkbox(&mut f.use_date, "");
                if f.use_date {
                    widgets::date_picker(ui, &mut f.date, "goal-date");
                } else {
                    ui.label(
                        weak(
                            ui,
                            tr!(
                                "sem data: o treinador estima quando dá",
                                "no date: the coach estimates when it is realistic"
                            ),
                        )
                        .small(),
                    );
                }
            });
            ui.end_row();
        });
    if let Some(e) = &f.error {
        ui.label(RichText::new(e).color(widgets::BAD));
    }
    ui.add_space(6.0);
    if ui.button(RichText::new(save_label).strong()).clicked() && f.apply() {
        saved = Some(f.g.clone());
    }
    saved
}

#[derive(Default)]
pub struct State {
    pub form: Option<Form>,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    widgets::page_title(ui, tr!("Meta", "Goal"));
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.set_max_width(860.0);
        let current = app.coach.as_ref().map(|c| c.goal.clone());
        if app.pages.goal.form.is_none() {
            app.pages.goal.form = Some(Form::new(current.clone().unwrap_or_default()));
        }
        if let Some(c) = &app.coach {
            widgets::card_frame(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(weak(ui, tr!("Meta atual", "Current goal")).small());
                ui.label(RichText::new(c.goal.describe()).size(20.0).strong());
                ui.label(weak(
                    ui,
                    trf!(
                        "{} dias por semana · desde {}",
                        "{} days a week · since {}",
                        c.goal.days(),
                        format::date(c.goal.start_date)
                    ),
                ));
            });
            ui.add_space(8.0);
        }
        widgets::section(ui, if current.is_some() { tr!("Alterar a meta", "Change the goal") } else { tr!("Defina sua meta", "Set your goal") });
        ui.label(weak(
            ui,
            tr!(
                "Combine o que quiser: só a distância (\"correr 10 km\"), distância e tempo (\"5 km em 28 min\"), ou distância e FC (\"5 km a 6:30/km com FC média até 160\"). Ao salvar, o plano é refeito a partir de hoje; os treinos já feitos ficam no histórico.",
                "Combine what you like: distance only (\"run 10 km\"), distance and time (\"5 km in 28 min\"), or distance and HR (\"5 km at 6:30/km with avg HR up to 160\"). When you save, the plan is rebuilt from today; runs already done stay in the history."
            ),
        ));
        ui.add_space(6.0);
        let f = app.pages.goal.form.as_mut().unwrap();
        if let Some(mut g) = form(ui, f, tr!("💾 Salvar meta e refazer o plano", "💾 Save goal and rebuild the plan")) {
            let today = chrono::Local::now().date_naive();
            g.start_date = today;
            if g.target_hr.is_some() && g.target_time_s.is_none() && g.hr_pace.is_none() {
                // HR goal without a pace: freeze the current easy pace as the reference
                g.hr_pace = app.coach.as_ref().map(|c| (c.fitness.easy_pace / 5.0).round() * 5.0).or(Some(crate::coach::fitness::BEGINNER_EASY_PACE));
            }
            match app.db.set_goal(&g) {
                Ok(_) => {
                    app.refresh_coach(true);
                    app.pages.goal.form = None;
                    app.toast(tr!("Meta salva. O plano foi refeito.", "Goal saved. The plan was rebuilt."));
                    app.go(Page::Plan);
                }
                Err(e) => app.toast_error(e.to_string()),
            }
            return;
        }
        let history = app.db.goal_history().unwrap_or_default();
        if history.len() > 1 || history.iter().any(|h| h.3.is_some()) {
            widgets::section(ui, tr!("Histórico de metas", "Goal history"));
            egui::Grid::new("goal-history").striped(true).num_columns(3).show(ui, |ui| {
                for (_, at, g, achieved) in &history {
                    let created = chrono::DateTime::from_timestamp(*at, 0).map(|d| d.date_naive()).unwrap_or_default();
                    ui.label(format::date(created));
                    ui.label(g.describe());
                    match achieved {
                        Some(d) => widgets::chip(ui, &trf!("✔ atingida em {}", "✔ reached on {}", d), widgets::GOOD),
                        None => {
                            ui.label("");
                        }
                    }
                    ui.end_row();
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_distance_is_limited_to_10_km() {
        let mut f = Form::new(Goal::default());
        f.distance = "10".into();
        assert!(f.apply());
        assert_eq!(f.g.distance_m, 10_000.0);
        for bad in ["10,5", "21,1", "42", "0", "abc"] {
            f.distance = bad.into();
            assert!(!f.apply(), "{bad}");
            assert!(f.error.is_some());
        }
    }
}
