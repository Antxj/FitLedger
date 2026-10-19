//! The generated schedule, week by week: planned vs done, phases, details.

use super::today::{kind_color, session_details, status_color};
use super::widgets::{self, weak};
use super::{App, Page};
use crate::coach::adapt::Tracked;
use crate::coach::{Status, week_start};
use crate::format;
use chrono::NaiveDate;
use eframe::egui::{self, RichText, Ui};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct State {
    show_past: bool,
    open: Option<i64>,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let today = chrono::Local::now().date_naive();
    ui.horizontal(|ui| {
        widgets::page_title(ui, tr!("Plano", "Plan"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr!("🎯 Alterar meta", "🎯 Change goal")).clicked() {
                app.go(Page::Goal);
            }
            ui.checkbox(
                &mut app.pages.plan.show_past,
                tr!("Mostrar semanas passadas", "Show past weeks"),
            );
        });
    });
    let Some(c) = app.coach.clone() else {
        widgets::empty_state(
            ui,
            tr!(
                "Defina uma meta para gerar o plano.",
                "Set a goal to generate the plan."
            ),
        );
        if ui.button(tr!("🎯 Definir meta", "🎯 Set goal")).clicked() {
            app.go(Page::Goal);
        }
        return;
    };
    ui.label(weak(
        ui,
        trf!(
            "{} · {} dias por semana. O plano é refeito sempre que você importa uma corrida ou muda a meta: o futuro se ajusta, o passado fica registrado.",
            "{} · {} days a week. The plan is rebuilt whenever you import a run or change the goal: the future adjusts, the past stays recorded.",
            c.goal.describe(),
            c.goal.days()
        ),
    ));
    egui::CollapsingHeader::new(tr!("Como o plano é montado", "How the plan is built")).show(ui, |ui| {
        for line in [
            tr!(
                "• Base aeróbica primeiro: só corridas leves (FC até o topo da zona 2) e um longão. É isso que baixa a FC no mesmo ritmo.",
                "• Aerobic base first: only easy runs (HR up to the top of zone 2) and a long run. That is what lowers HR at the same pace."
            ),
            tr!(
                "• O tempo semanal cresce no máximo 8% por semana; a cada 4 semanas, uma semana mais leve.",
                "• Weekly time grows at most 8% a week; every 4th week is lighter."
            ),
            tr!("• O longão cresce cerca de 10% por semana até a distância da meta.", "• The long run grows about 10% a week up to the goal distance."),
            tr!(
                "• Depois da base: um treino de qualidade por semana (intervalado no pace da meta se houver tempo-alvo; ritmo moderado nos demais casos).",
                "• After the base: one quality session a week (intervals at goal pace with a time goal; tempo otherwise)."
            ),
            tr!(
                "• A cada 3 semanas, um teste de evolução no mesmo pace, para medir a FC.",
                "• Every 3 weeks, a progress check at the same pace, to measure HR."
            ),
            tr!(
                "• Treinos perdidos não são compensados; FC alta nas corridas leves, deriva alta ou cansaço deixam o plano mais cauteloso.",
                "• Missed sessions are not made up; high HR on easy runs, high drift or fatigue make the plan more cautious."
            ),
        ] {
            ui.label(line);
        }
    });
    egui::CollapsingHeader::new(tr!("Base científica", "Scientific basis")).show(ui, |ui| {
        ui.label(weak(
            ui,
            tr!(
                "Os métodos e estudos em que o treinador se apoia, e o que é regra própria do app. Orientação de treino, não é avaliação médica.",
                "The methods and studies the coach relies on, and what is the app's own rule. Training guidance, not medical advice."
            ),
        ));
        for b in crate::coach::science::BASIS {
            ui.add_space(4.0);
            ui.label(RichText::new(b.topic()).strong());
            ui.label(b.usage());
            if !b.reference.is_empty() {
                ui.label(weak(ui, b.reference).small().italics());
            }
        }
    });
    let log = app.db.coach_log(c.goal.id, 20).unwrap_or_default();
    if !log.is_empty() {
        egui::CollapsingHeader::new(trf!("Histórico de ajustes ({})", "Adjustment history ({})", log.len())).show(
            ui,
            |ui| {
                for (at, notes, evidence) in &log {
                    let when = chrono::DateTime::from_timestamp(*at, 0)
                        .map(|d| format::datetime(d.with_timezone(&chrono::Local).naive_local()))
                        .unwrap_or_default();
                    ui.label(RichText::new(when).strong());
                    for n in notes {
                        ui.label(format!("• {}", n.text()));
                    }
                    for e in evidence {
                        ui.label(weak(ui, format!("   {e}")).small());
                    }
                }
            },
        );
    }
    ui.add_space(4.0);
    let mut weeks: BTreeMap<NaiveDate, Vec<&Tracked>> = BTreeMap::new();
    for t in &c.sessions {
        weeks.entry(week_start(t.session.date)).or_default().push(t);
    }
    let this_week = week_start(today);
    let show_past = app.pages.plan.show_past;
    let mut open_run = None;
    let mut toggle = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (ws, list) in &weeks {
            if *ws < this_week && !show_past {
                continue;
            }
            let first = &list[0].session;
            let minutes: f64 = list.iter().map(|t| t.session.duration_s).sum::<f64>() / 60.0;
            let km: f64 = list.iter().map(|t| t.session.distance_m).sum::<f64>() / 1000.0;
            let past: Vec<&&Tracked> = list.iter().filter(|t| t.status != Status::Planned).collect();
            let done = past
                .iter()
                .filter(|t| matches!(t.status, Status::Done | Status::Partial))
                .count();
            widgets::card_frame(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(trf!("Semana {}", "Week {}", first.week))
                            .strong()
                            .size(16.0),
                    );
                    ui.label(weak(
                        ui,
                        format!(
                            "{} – {}",
                            format::date_short(*ws),
                            format::date_short(*ws + chrono::Duration::days(6))
                        ),
                    ));
                    widgets::chip(ui, first.phase.label(), widgets::ACCENT);
                    if *ws == this_week {
                        widgets::chip(ui, tr!("semana atual", "this week"), widgets::WARN);
                    }
                    ui.label(weak(
                        ui,
                        format!(
                            "{} · ≈ {}",
                            format::hours_short(minutes * 60.0),
                            format::km(km * 1000.0)
                        ),
                    ));
                    if !past.is_empty() {
                        ui.label(weak(ui, trf!("· {done}/{} feitos", "· {done}/{} done", past.len())));
                    }
                });
                for t in list {
                    let s = &t.session;
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [70.0, 18.0],
                            egui::Label::new(format!("{} {}", format::weekday_short(s.date), s.date.format("%d"))),
                        );
                        let open = app.pages.plan.open == Some(t.id);
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(format!("{} {}", if open { "▼" } else { "▶" }, s.kind.label()))
                                        .color(kind_color(s.kind)),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            toggle = Some(t.id);
                        }
                        let mut txt = format!("{} · ≈ {}", format::duration(s.duration_s), format::km(s.distance_m));
                        if let Some(p) = s.pace {
                            txt.push_str(&format!(" · {}–{}/km", format::pace(p.0), format::pace(p.1)));
                        }
                        if let Some(h) = s.hr_max {
                            txt.push_str(&format!(" · ≤ {} bpm", h.round()));
                        }
                        ui.label(weak(ui, txt));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(id) = t.activity_id
                                && ui.small_button(tr!("ver corrida", "see run")).clicked()
                            {
                                open_run = Some(id);
                            }
                            widgets::chip(ui, t.status.label(), status_color(t.status));
                        });
                    });
                    if app.pages.plan.open == Some(t.id) {
                        ui.indent(("d", t.id), |ui| session_details(ui, t));
                    }
                }
            });
            ui.add_space(4.0);
        }
    });
    if let Some(id) = toggle {
        app.pages.plan.open = if app.pages.plan.open == Some(id) {
            None
        } else {
            Some(id)
        };
    }
    if let Some(id) = open_run {
        app.go(Page::Activity(id));
    }
}
