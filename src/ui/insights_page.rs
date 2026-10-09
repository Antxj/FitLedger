//! Weekly summary (item 27) and automatic insights with their evidence (item 28).

use super::App;
use super::widgets::{self, weak};
use crate::analysis::insights::{self, Input, Tone};
use crate::analysis::load::{self, week_start};
use crate::format;
use chrono::Duration;
use eframe::egui::{self, RichText, Ui};

#[derive(Default)]
pub struct State {
    /// Weeks back from the current one.
    week_offset: i64,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    widgets::page_title(ui, tr!("Insights", "Insights"));
    let today = chrono::Local::now().date_naive();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        weekly(app, ui, today);
        widgets::section(ui, tr!("Insights automáticos", "Automatic insights"));
        ui.label(weak(
            ui,
            tr!(
                "Conclusões simples tiradas dos seus dados. Abra cada uma para ver os dados que levaram a ela.",
                "Simple conclusions drawn from your data. Open each one to see the data behind it."
            ),
        ));
        let tests = app.db.reference_tests().unwrap_or_default();
        let ref_tests = tests
            .iter()
            .map(|t| (t.name.clone(), app.db.reference_runs(t.id).unwrap_or_default()))
            .collect();
        let list = insights::generate(&Input {
            acts: &app.data.activities,
            metrics: &app.data.metrics,
            ref_tests,
            today,
        });
        if list.is_empty() {
            widgets::empty_state(
                ui,
                tr!(
                    "Ainda não há dados suficientes. Os insights aparecem conforme o histórico cresce.",
                    "Not enough data yet. Insights show up as the history grows."
                ),
            );
        }
        for (i, ins) in list.iter().enumerate() {
            let (icon, color) = match ins.tone {
                Tone::Good => ("✅", widgets::GOOD),
                Tone::Neutral => ("ℹ", widgets::ACCENT),
                Tone::Attention => ("⚠", widgets::WARN),
            };
            widgets::card_frame(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(icon).color(color).size(16.0));
                    ui.label(RichText::new(&ins.text).size(15.0));
                });
                egui::CollapsingHeader::new(trf!("Dados ({})", "Data ({})", ins.evidence.len()))
                    .id_salt(("ins", i))
                    .show(ui, |ui| {
                        for e in &ins.evidence {
                            ui.label(weak(ui, format!("• {e}")).small());
                        }
                    });
            });
            ui.add_space(4.0);
        }
    });
}

fn weekly(app: &mut App, ui: &mut Ui, today: chrono::NaiveDate) {
    let off = &mut app.pages.insights.week_offset;
    let ws = week_start(today) - Duration::days(7 * *off);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr!("Resumo semanal", "Weekly summary"))
                .size(17.0)
                .strong(),
        );
        if ui.button("◀").clicked() {
            *off += 1;
        }
        ui.label(format!(
            "{} – {}",
            format::date_short(ws),
            format::date(ws + Duration::days(6))
        ));
        if ui.add_enabled(*off > 0, egui::Button::new("▶")).clicked() {
            *off -= 1;
        }
        if *off == 0 {
            widgets::chip(ui, tr!("semana atual", "current week"), widgets::ACCENT);
        }
    });
    let acts = &app.data.activities;
    let m = &app.data.metrics;
    let s = load::week_summary(acts, m, ws);
    let prev = load::week_summary(acts, m, ws - Duration::days(7));
    let prev4 = (1..=4)
        .map(|k| load::week_summary(acts, m, ws - Duration::days(7 * k)).load)
        .sum::<f64>()
        / 4.0;
    let cmp = |a: f64, b: f64| {
        if b > 0.0 {
            format::pct_signed((a - b) / b * 100.0)
        } else {
            format::dash()
        }
    };
    widgets::card_grid(
        ui,
        150.0,
        &[
            (
                tr!("Corrida", "Running").into(),
                format!("{} km", format::num(s.run_km, 1)),
                Some(trf!(
                    "{} corridas · {}",
                    "{} runs · {}",
                    s.run_count,
                    cmp(s.run_km, prev.run_km)
                )),
            ),
            (tr!("Tênis", "Tennis").into(), format::hours_short(s.tennis_s), None),
            (
                tr!("Musculação", "Strength").into(),
                trf!("{} sessões", "{} sessions", s.strength_sessions),
                None,
            ),
            (
                tr!("Tempo total", "Total time").into(),
                format::hours_short(s.total_s),
                Some(trf!(
                    "{} vs. semana anterior",
                    "{} vs. previous week",
                    cmp(s.total_s, prev.total_s)
                )),
            ),
            (
                tr!("Corrida longa", "Long run").into(),
                format!("{} km", format::num(s.longest_run_km, 1)),
                None,
            ),
            (
                tr!("FC média em corrida fácil", "Avg HR on easy runs").into(),
                format::bpm(s.easy_hr),
                None,
            ),
            (
                tr!("Carga", "Load").into(),
                load::load_class(s.load, prev4).into(),
                Some(format!(
                    "{} ({} {})",
                    s.load.round(),
                    tr!("média 4 sem.:", "4-week avg:"),
                    prev4.round()
                )),
            ),
        ],
    );
}
