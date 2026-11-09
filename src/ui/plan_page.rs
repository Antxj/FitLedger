//! The plan: the queue of next sessions, then what was done, week by week.

use super::today::{kind_color, session_details, status_color};
use super::widgets::{self, weak};
use super::{App, Page};
use crate::coach::adapt::Tracked;
use crate::coach::week_start;
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
                tr!("Mostrar todas as semanas", "Show all weeks"),
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
            "{} · sem dias fixos. Cada corrida importada cumpre o próximo treino; a fila é refeita a cada corrida ou mudança de meta, e o que foi feito fica registrado.",
            "{} · no fixed days. Each imported run fulfils the next session; the queue is rebuilt after every run or goal change, and what was done stays recorded.",
            c.goal.describe()
        ),
    ));
    egui::CollapsingHeader::new(tr!("Como o plano é montado", "How the plan is built")).show(ui, |ui| {
        for line in [
            tr!(
                "• Sem dias fixos: você corre quando puder e cada corrida cumpre o próximo treino da fila. Nada fica \"perdido\".",
                "• No fixed days: you run whenever you can and each run fulfils the next session of the queue. Nothing is \"missed\"."
            ),
            tr!(
                "• O tamanho dos treinos vem do que você realmente corre (a mediana das últimas 3 corridas): a corrida leve cresce até 5% e o longão até 10% de cada vez, até a distância da meta.",
                "• Sessions are sized from what you actually run (the median of your last 3 runs): the easy run grows up to 5% and the long run up to 10% at a time, up to the goal distance."
            ),
            tr!(
                "• Base aeróbica primeiro: só corridas leves (FC até o topo da zona 2) e longões. É isso que baixa a FC no mesmo ritmo. Se o ritmo leve for muito lento, a corrida leve vira corrida + caminhada.",
                "• Aerobic base first: only easy runs (HR up to the top of zone 2) and long runs. That is what lowers HR at the same pace. If the easy pace is very slow, easy runs become run + walk."
            ),
            tr!(
                "• Depois da base, a cada 4 treinos: leve, longão, leve e um treino especial: teste de evolução a cada ~18 dias; intervalado ou ritmo moderado só para quem corre 2 ou mais vezes por semana.",
                "• After the base, every 4 sessions: easy, long, easy and a special one: a progress check every ~18 days; intervals or tempo only when running 2 or more times a week."
            ),
            tr!(
                "• A cada 3 ciclos de 4 treinos, um ciclo mais leve. Depois de um treino forte, um dia de descanso.",
                "• Every third cycle of 4 sessions is lighter. After a hard session, a rest day."
            ),
            tr!(
                "• FC alta nas corridas leves, deriva alta, cansaço, muitos minutos numa semana ou uma pausa longa deixam o próximo treino mais cauteloso.",
                "• High HR on easy runs, high drift, fatigue, a big week or a long break make the next session more cautious."
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
    let sick = crate::coach::game::load_sick_weeks(&app.db);
    let mut toggle_sick: Option<NaiveDate> = None;
    let this_week = week_start(today);
    let show_all = app.pages.plan.show_past;
    let mut open_run = None;
    let mut toggle = None;
    // done sessions by week, from the goal start (empty weeks included)
    let mut weeks: BTreeMap<NaiveDate, Vec<&Tracked>> = BTreeMap::new();
    let mut ws = week_start(c.goal.start_date);
    while ws <= this_week {
        weeks.insert(ws, Vec::new());
        ws += chrono::Duration::days(7);
    }
    for t in c.sessions.iter().filter(|t| t.activity_id.is_some()) {
        weeks.entry(week_start(t.session.date)).or_default().push(t);
    }
    let queue = c.queue();
    let mut row = |ui: &mut Ui, t: &Tracked, label: String| {
        let s = &t.session;
        ui.horizontal(|ui| {
            ui.add_sized([96.0, 18.0], egui::Label::new(label));
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
            if s.run_walk() {
                txt.push_str(tr!(" · corrida + caminhada", " · run + walk"));
            } else if let Some(p) = s.pace {
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
                if t.activity_id.is_some() {
                    widgets::chip(ui, t.status.label(), status_color(t.status));
                } else {
                    widgets::chip(ui, s.phase.label(), widgets::ACCENT);
                }
                if let Some(sc) = app.game.as_ref().and_then(|g| g.scores.get(&t.id)) {
                    ui.label(RichText::new(super::game_ui::stars_text(sc.stars())).color(super::game_ui::GOLD))
                        .on_hover_text(sc.reasons.join("\n"));
                }
            });
        });
        if app.pages.plan.open == Some(t.id) {
            ui.indent(("d", t.id), |ui| {
                session_details(ui, t);
                if let Some(sc) = app.game.as_ref().and_then(|g| g.scores.get(&t.id)) {
                    super::game_ui::score_details(ui, sc);
                }
            });
        }
    };
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(tr!("Próximos treinos", "Next sessions"))
                    .strong()
                    .size(16.0),
            );
            for (i, t) in queue.iter().enumerate() {
                let label = match i {
                    0 if t.session.date > today => {
                        trf!("a partir de {}", "from {}", format::date_short(t.session.date))
                    }
                    0 => tr!("próximo", "next").into(),
                    1 => tr!("depois", "then").into(),
                    _ => tr!("em seguida", "after that").into(),
                };
                row(ui, t, label);
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new(tr!("Feitos", "Done")).strong().size(16.0));
        for (n, (ws, list)) in weeks.iter().rev().enumerate() {
            if n >= 4 && !show_all {
                ui.label(weak(
                    ui,
                    tr!(
                        "Semanas mais antigas: marque \"Mostrar todas as semanas\".",
                        "Older weeks: tick \"Show all weeks\"."
                    ),
                ));
                break;
            }
            widgets::card_frame(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} – {}",
                            format::date_short(*ws),
                            format::date_short(*ws + chrono::Duration::days(6))
                        ))
                        .strong(),
                    );
                    if *ws == this_week {
                        widgets::chip(ui, tr!("semana atual", "this week"), widgets::WARN);
                    }
                    if sick.contains(ws) {
                        widgets::chip(ui, tr!("doente/viagem", "sick/travel"), egui::Color32::GRAY);
                    }
                    let minutes: f64 = list
                        .iter()
                        .filter_map(|t| t.activity_id.and_then(|id| app.data.get(id)))
                        .filter_map(|a| a.duration())
                        .sum::<f64>();
                    ui.label(weak(
                        ui,
                        if list.is_empty() {
                            tr!("nenhum treino", "no sessions").to_string()
                        } else {
                            trf!(
                                "{} treino(s) · {}",
                                "{} session(s) · {}",
                                list.len(),
                                format::hours_short(minutes)
                            )
                        },
                    ));
                    if (list.is_empty() || sick.contains(ws))
                        && ui
                            .small_button(if sick.contains(ws) {
                                tr!("desmarcar doente/viagem", "unmark sick/travel")
                            } else {
                                tr!("doente/viagem", "sick/travel")
                            })
                            .on_hover_text(tr!(
                                "Semana em que você ficou doente ou viajou: ela não quebra a sequência.",
                                "A week when you were sick or travelling: it does not break the streak."
                            ))
                            .clicked()
                    {
                        toggle_sick = Some(*ws);
                    }
                });
                for t in list {
                    row(
                        ui,
                        t,
                        format!(
                            "{} {}",
                            format::weekday_short(t.session.date),
                            t.session.date.format("%d")
                        ),
                    );
                }
            });
            ui.add_space(4.0);
        }
    });
    if let Some(w) = toggle_sick {
        crate::coach::game::toggle_sick_week(&app.db, w);
        app.refresh_coach(false);
    }
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
