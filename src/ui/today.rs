//! Home: the next session (and when), goal progress, the queue, coach notes,
//! last run.

use super::widgets::{self, weak};
use super::{App, Page};
use crate::coach::adapt::Tracked;
use crate::coach::{SessionKind, Status};
use crate::format;
use eframe::egui::{self, Color32, RichText, Ui};

pub fn kind_color(k: SessionKind) -> Color32 {
    match k {
        SessionKind::Easy => Color32::from_rgb(0x22, 0xc5, 0x5e),
        SessionKind::Long => Color32::from_rgb(0x3b, 0x82, 0xf6),
        SessionKind::Tempo => Color32::from_rgb(0xf5, 0x9e, 0x0b),
        SessionKind::Intervals => Color32::from_rgb(0xef, 0x44, 0x44),
        SessionKind::Check => Color32::from_rgb(0xa8, 0x55, 0xf7),
        SessionKind::GoalAttempt => Color32::from_rgb(0xec, 0x48, 0x99),
    }
}

pub fn status_color(s: Status) -> Color32 {
    match s {
        Status::Planned => widgets::ACCENT,
        Status::Done => widgets::GOOD,
        Status::Partial => widgets::WARN,
        Status::Missed => widgets::BAD,
    }
}

/// Full description of a session: targets and steps.
pub fn session_details(ui: &mut Ui, t: &Tracked) {
    let s = &t.session;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format::duration(s.duration_s)).size(18.0).strong());
        ui.label(RichText::new(format!("≈ {}", format::km(s.distance_m))).size(16.0));
        if let Some(h) = s.hr_max {
            widgets::chip(ui, &trf!("FC ≤ {} bpm", "HR ≤ {} bpm", h.round()), widgets::HR);
            widgets::help(
                ui,
                tr!(
                    "Limite de FC desta sessão. Nas corridas leves é o topo da zona 2 (calculada pela sua FC de reserva): ficar abaixo dele é o que melhora a base aeróbica e baixa a FC no mesmo ritmo.",
                    "This session's HR limit. On easy runs it is the top of zone 2 (from your heart-rate reserve): staying under it is what builds the aerobic base and lowers HR at the same pace."
                ),
            );
        }
        widgets::chip(
            ui,
            if s.treadmill {
                tr!("sugestão: esteira", "suggested: treadmill")
            } else {
                tr!("sugestão: rua", "suggested: road")
            },
            Color32::GRAY,
        );
    });
    if let Some(p) = s.pace_text() {
        let text = if s.run_walk() {
            trf!(
                "Pace médio, com as caminhadas: {}",
                "Average pace, walks included: {}",
                p
            )
        } else {
            trf!("Pace: {}", "Pace: {}", p)
        };
        ui.label(RichText::new(text).color(widgets::SPEED));
    }
    // one wrapped line per block (a grid would not wrap and widens the column)
    for st in &s.steps {
        let mut parts = vec![format!(
            "{}{} · {}",
            if st.repeat > 1 {
                format!("{}× ", st.repeat)
            } else {
                String::new()
            },
            st.label(),
            format::duration(st.duration_s)
        )];
        if let Some((a, b)) = st.pace {
            parts.push(format!(
                "{}–{}/km · {}–{} km/h",
                format::pace(a),
                format::pace(b),
                format::num(format::pace_to_kmh(b), 1),
                format::num(format::pace_to_kmh(a), 1)
            ));
        }
        if let Some(h) = st.hr_max {
            parts.push(format!("≤ {} bpm", h.round()));
        }
        ui.label(RichText::new(format!("• {}", parts.join(" · "))).small());
    }
    let run_walk_tip = s.run_walk().then(|| {
        trf!(
            "Corrida + caminhada: trote leve com FC até {} bpm e caminhe 1 minuto entre os blocos (se a FC passar antes, caminhe antes). É o que deixa o coração trabalhar na zona certa enquanto ele fica mais eficiente; as caminhadas diminuem conforme a FC no mesmo ritmo cai.",
            "Run + walk: easy jog with HR up to {} bpm and a 1-minute walk between blocks (walk earlier if HR goes over). It keeps the heart in the right zone while it gets more efficient; walks shrink as HR at the same pace drops.",
            s.hr_max.unwrap_or(0.0).round()
        )
    });
    let tip = match s.kind {
        _ if run_walk_tip.is_some() => run_walk_tip.as_deref().unwrap_or_default(),
        SessionKind::Easy => tr!(
            "Devagar de verdade: dá para conversar. Se a FC passar do limite, diminua o ritmo ou caminhe um pouco.",
            "Truly easy: you can talk. If HR goes over the limit, slow down or walk for a bit."
        ),
        SessionKind::Long => tr!(
            "Mesmo esforço da corrida leve, só que mais longo. A distância importa mais que o ritmo.",
            "Same effort as an easy run, just longer. Distance matters more than pace."
        ),
        SessionKind::Tempo => tr!(
            "Confortavelmente difícil: frases curtas, não conversa.",
            "Comfortably hard: short phrases, no chatting."
        ),
        SessionKind::Intervals => tr!(
            "Os blocos fortes no pace da meta; recuperações bem leves. Na esteira, ajuste a velocidade pela tabela.",
            "Hard blocks at goal pace; very easy recoveries. On the treadmill, set the speed from the table."
        ),
        SessionKind::Check => tr!(
            "Teste para medir a evolução: mesmo pace sempre, de preferência na esteira com a mesma inclinação. O app compara a FC com os testes anteriores.",
            "A test to measure progress: always the same pace, ideally on the treadmill at the same incline. The app compares HR with previous checks."
        ),
        SessionKind::GoalAttempt => tr!(
            "A tentativa da meta: descansado, comece controlado e acelere no fim se der.",
            "The goal attempt: rested, start controlled and speed up at the end if you can."
        ),
    };
    ui.label(weak(ui, tip).small().italics());
}

pub fn show(app: &mut App, ui: &mut Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| content(app, ui));
}

fn content(app: &mut App, ui: &mut Ui) {
    let today = chrono::Local::now().date_naive();
    ui.label(
        RichText::new(format!("{} {}", format::weekday_short(today), format::date(today)))
            .color(ui.visuals().weak_text_color()),
    );
    widgets::page_title(ui, tr!("Hoje", "Today"));
    let Some(c) = app.coach.clone() else {
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(tr!("Defina sua meta para receber um plano.", "Set your goal to get a plan.")).size(18.0).strong());
            ui.label(weak(
                ui,
                tr!(
                    "Diga a distância e, se quiser, o tempo e/ou a FC média. Sem dias fixos: o FitLedger propõe o próximo treino e o ajusta a cada corrida importada.",
                    "Tell the distance and, optionally, the time and/or the average HR. No fixed days: FitLedger proposes the next session and adjusts it after every imported run."
                ),
            ));
            if ui.button(RichText::new(tr!("🎯 Definir meta", "🎯 Set goal")).strong()).clicked() {
                app.go(Page::Goal);
            }
        });
        last_run(app, ui, None);
        return;
    };

    if let Some((d, _)) = c.reached {
        widgets::note(
            ui,
            &trf!(
                "🏆 Meta atingida em {}! Defina uma meta nova quando quiser.",
                "🏆 Goal reached on {}! Set a new goal whenever you like.",
                format::date(d)
            ),
            widgets::GOOD,
        );
    }

    let done_today: Vec<Tracked> = c.done_on(today).into_iter().cloned().collect();
    let mut open_run = None;
    ui.columns(2, |cols| {
        // the next session, and when
        let ui = &mut cols[0];
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for t in &done_today {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(trf!("✔ Hoje: {} feito", "✔ Today: {} done", t.session.kind.label()))
                            .color(widgets::GOOD)
                            .strong(),
                    );
                    if let Some(sc) = app.game.as_ref().and_then(|g| g.scores.get(&t.id)) {
                        ui.label(RichText::new(super::game_ui::stars_text(sc.stars())).color(super::game_ui::GOLD));
                    }
                    if let Some(id) = t.activity_id
                        && ui.link(tr!("ver", "see")).clicked()
                    {
                        open_run = Some(id);
                    }
                });
            }
            match c.next_session() {
                None => {
                    ui.label(weak(ui, tr!("Nenhum treino planejado.", "No session planned.")));
                }
                Some(t) => {
                    let after_hard = done_today
                        .iter()
                        .any(|d| d.session.kind == SessionKind::Long || d.session.kind.is_hard());
                    let when = if t.session.date <= today {
                        tr!(
                            "Próximo treino · quando puder, hoje já dá",
                            "Next session · whenever you can, today is fine"
                        )
                        .to_string()
                    } else if t.session.date == today + chrono::Duration::days(1) {
                        if after_hard || !done_today.is_empty() {
                            tr!(
                                "Próximo treino · a partir de amanhã (hoje, descanso)",
                                "Next session · from tomorrow (rest today)"
                            )
                            .to_string()
                        } else {
                            tr!("Próximo treino · a partir de amanhã", "Next session · from tomorrow").to_string()
                        }
                    } else {
                        trf!(
                            "Próximo treino · a partir de {} {} (descanso antes)",
                            "Next session · from {} {} (rest first)",
                            format::weekday_short(t.session.date),
                            format::date_short(t.session.date)
                        )
                    };
                    ui.label(weak(ui, when).small());
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(t.session.kind.label())
                                .size(22.0)
                                .strong()
                                .color(kind_color(t.session.kind)),
                        );
                        widgets::chip(ui, t.session.phase.label(), widgets::ACCENT);
                    });
                    session_details(ui, t);
                }
            }
        });
        // goal progress
        let ui = &mut cols[1];
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(weak(ui, tr!("Meta", "Goal")).small());
            ui.label(RichText::new(c.goal.describe()).size(17.0).strong());
            for p in &c.projection.parts {
                ui.label(RichText::new(&p.label).small());
                widgets::progress(ui, p.fraction, &format!("{} → {}", p.now, p.goal), p.done);
            }
            if c.reached.is_none() {
                let mut est = trf!(
                    "Estimativa: {} ({} semanas)",
                    "Estimate: {} ({} weeks)",
                    format::date(c.projection.date),
                    c.projection.weeks
                );
                if let Some(d) = c.goal.target_date {
                    est.push_str(&trf!(" · data alvo {}", " · target date {}", format::date(d)));
                }
                ui.label(RichText::new(est).strong());
                ui.label(
                    weak(
                        ui,
                        tr!(
                            "Estimativa conservadora, refeita a cada corrida.",
                            "Conservative estimate, redone after every run."
                        ),
                    )
                    .small(),
                );
            }
        });
    });

    ui.add_space(8.0);
    if let Some(g) = app.game.clone() {
        ui.columns(2, |cols| {
            super::game_ui::level_card(&mut cols[0], &g);
            super::game_ui::missions_card(&mut cols[1], &g);
        });
        ui.add_space(8.0);
    }
    super::game_ui::journey(ui, &c, &app.data.activities);
    ui.add_space(8.0);
    timeline(app, ui, &c);
    ui.add_space(8.0);
    // coach notes
    if !c.adjust.notes.is_empty() {
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(weak(ui, tr!("O treinador diz", "Coach notes")).small());
            for n in &c.adjust.notes {
                let color = if n.is_warning() { widgets::WARN } else { widgets::ACCENT };
                ui.label(RichText::new(format!("• {}", n.text())).color(color));
            }
            if !c.adjust.evidence.is_empty() {
                egui::CollapsingHeader::new(trf!("Dados ({})", "Data ({})", c.adjust.evidence.len())).show(ui, |ui| {
                    for e in &c.adjust.evidence {
                        ui.label(weak(ui, format!("• {e}")).small());
                    }
                });
            }
        });
        ui.add_space(8.0);
    }
    ui.columns(2, |cols| {
        last_run(app, &mut cols[0], Some(&c));
        fitness_card(&mut cols[1], &c);
    });
    if let Some(id) = open_run {
        app.go(Page::Activity(id));
    }
}

/// The last plan sessions done and the next ones, left to right.
fn timeline(app: &mut App, ui: &mut Ui, c: &crate::coach::store::CoachState) {
    let mut done: Vec<&Tracked> = c.sessions.iter().filter(|t| t.activity_id.is_some()).collect();
    done.sort_by_key(|t| (t.session.date, t.id));
    let done: Vec<&Tracked> = done.into_iter().rev().take(2).rev().collect();
    let queue = c.queue();
    let mut open = None;
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(weak(ui, tr!("Treinos", "Sessions")).small());
            widgets::help(
                ui,
                tr!(
                    "Sem dias fixos: corra quando puder. Cada corrida importada cumpre o próximo treino da fila e a fila é refeita com base no que você fez.",
                    "No fixed days: run whenever you can. Each imported run fulfils the next session of the queue, and the queue is rebuilt from what you did."
                ),
            );
            if ui.link(tr!("ver plano completo →", "see the full plan →")).clicked() {
                app.go(Page::Plan);
            }
        });
        let n = (done.len() + queue.len()).max(1);
        let w = (ui.available_width() - (n as f32 - 1.0) * 8.0) / n as f32;
        ui.horizontal(|ui| {
            for (i, t) in done.iter().chain(queue.iter()).enumerate() {
                let is_done = t.activity_id.is_some();
                let next = !is_done && i == done.len();
                egui::Frame::new()
                    .fill(if next {
                        widgets::ACCENT.gamma_multiply(0.15)
                    } else {
                        ui.visuals().faint_bg_color
                    })
                    .corner_radius(8.0)
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        ui.set_width(w - 12.0);
                        ui.set_min_height(64.0);
                        ui.vertical(|ui| {
                            let head = if is_done {
                                trf!(
                                    "Feito · {} {}",
                                    "Done · {} {}",
                                    format::weekday_short(t.session.date),
                                    format::date_short(t.session.date)
                                )
                            } else if next {
                                tr!("Próximo", "Next").into()
                            } else if i == done.len() + 1 {
                                tr!("Depois", "Then").into()
                            } else {
                                tr!("Em seguida", "After that").into()
                            };
                            ui.label(RichText::new(head).small().strong());
                            ui.label(
                                RichText::new(format!(
                                    "{} · {}",
                                    t.session.kind.label(),
                                    format::duration(t.session.duration_s)
                                ))
                                .small()
                                .color(kind_color(t.session.kind)),
                            );
                            if is_done {
                                ui.horizontal(|ui| {
                                    if let Some(sc) = app.game.as_ref().and_then(|g| g.scores.get(&t.id)) {
                                        ui.label(
                                            RichText::new(super::game_ui::stars_text(sc.stars()))
                                                .small()
                                                .color(super::game_ui::GOLD),
                                        );
                                    }
                                    if ui.small_button(tr!("abrir", "open")).clicked() {
                                        open = t.activity_id;
                                    }
                                });
                            } else {
                                ui.label(weak(ui, format!("≈ {}", format::km(t.session.distance_m))).small());
                            }
                        });
                    });
            }
        });
    });
    if let Some(id) = open {
        app.go(Page::Activity(id));
    }
}

fn last_run(app: &mut App, ui: &mut Ui, c: Option<&crate::coach::store::CoachState>) {
    let Some(a) = app.data.activities.first().cloned() else {
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(weak(
                ui,
                tr!(
                    "Nenhuma corrida ainda. Coloque os arquivos .fit na pasta \"fits\" ao lado do FitLedger.exe, ou use \"Importar arquivos FIT\".",
                    "No runs yet. Put .fit files in the \"fits\" folder next to FitLedger.exe, or use \"Import FIT files\"."
                ),
            ));
        });
        return;
    };
    let planned = c.and_then(|c| c.sessions.iter().find(|t| t.activity_id == Some(a.id)));
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(weak(ui, tr!("Última corrida", "Last run")).small());
        ui.horizontal(|ui| {
            ui.label(RichText::new(format::datetime(a.local_start())).strong());
            widgets::modality_chip(ui, a.modality);
        });
        ui.label(
            RichText::new(format!(
                "{} · {} · {} · {}",
                format::opt_km(a.distance()),
                format::opt_duration(a.duration()),
                format::opt_pace(a.pace_s_per_km()),
                format::bpm(a.avg_hr)
            ))
            .size(15.0),
        );
        if let Some(t) = planned {
            let s = &t.session;
            let mut verdict = trf!(
                "Plano: {} de {}",
                "Plan: {} of {}",
                s.kind.label(),
                format::duration(s.duration_s)
            );
            if let (Some(cap), Some(m)) = (s.hr_max, app.data.metrics(a.id)) {
                let total: f64 = m.zones_s.iter().sum();
                if total > 0.0 && matches!(s.kind, SessionKind::Easy | SessionKind::Long) {
                    let above = (m.zones_s[3] + m.zones_s[4] + m.zones_s[5]) / total * 100.0;
                    verdict.push_str(&trf!(
                        " · {}% do tempo acima de {} bpm",
                        " · {}% of the time above {} bpm",
                        format::num(above, 0),
                        cap.round()
                    ));
                }
            }
            ui.label(RichText::new(verdict).color(status_color(t.status)));
        }
        if ui.link(tr!("Abrir →", "Open →")).clicked() {
            app.go(Page::Activity(a.id));
        }
    });
}

fn fitness_card(ui: &mut Ui, c: &crate::coach::store::CoachState) {
    let f = &c.fitness;
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(weak(ui, tr!("Seu momento (últimas 6 semanas)", "Where you are (last 6 weeks)")).small());
            widgets::help(
                ui,
                tr!(
                    "Ritmo leve: o pace em que sua FC fica no topo da zona 2, tirado da relação FC × velocidade das suas corridas.
Tempo previsto: fórmula de Riegel a partir das suas melhores corridas recentes.
FC estimada: a FC que você teria hoje no pace da meta; a tendência mostra quanto ela cai por semana.",
                    "Easy pace: the pace at which your HR sits at the top of zone 2, from the HR × speed relation of your runs.
Predicted time: Riegel's formula from your best recent runs.
Estimated HR: the HR you would have today at goal pace; the trend shows how much it drops per week."
                ),
            );
        });
        egui::Grid::new("fitness")
            .num_columns(2)
            .spacing([16.0, 3.0])
            .show(ui, |ui| {
                let row = |ui: &mut Ui, k: &str, v: String| {
                    ui.label(weak(ui, k));
                    ui.label(RichText::new(v).strong());
                    ui.end_row();
                };
                row(
                    ui,
                    tr!("Ritmo leve (limite da zona 2)", "Easy pace (zone 2 limit)"),
                    format!(
                        "{} · {} km/h · ≤ {} bpm",
                        format::pace_unit(f.easy_pace),
                        format::num(format::pace_to_kmh(f.easy_pace), 1),
                        f.easy_hr_cap.round()
                    ),
                );
                row(
                    ui,
                    tr!("Volume semanal", "Weekly volume"),
                    format!(
                        "{} km · {}",
                        format::num(f.weekly_km, 1),
                        format::hours_short(f.weekly_minutes * 60.0)
                    ),
                );
                row(
                    ui,
                    tr!("Maior corrida (28 dias)", "Longest run (28 days)"),
                    format::km(f.longest_m),
                );
                row(
                    ui,
                    tr!("Corridas (28 dias)", "Runs (28 days)"),
                    trf!(
                        "{} · ~{}×/semana",
                        "{} · ~{}×/week",
                        f.runs_28d,
                        format::num(f.runs_per_week, 1)
                    ),
                );
                if f.typical_s > 0.0 {
                    row(ui, tr!("Treino típico (últimas 3)", "Typical run (last 3)"), format::duration(f.typical_s));
                }
                if let Some(p) = f.predicted_time {
                    row(
                        ui,
                        &trf!(
                            "Tempo previsto em {}",
                            "Predicted {} time",
                            format::km(c.goal.distance_m)
                        ),
                        format::duration(p),
                    );
                }
                if let (Some(h), Some(p)) = (f.hr_at_goal_pace, c.goal.goal_pace()) {
                    let trend = f
                        .hr_trend_per_week
                        .map(|t| trf!(" ({} bpm/semana)", " ({} bpm/week)", format::num(t, 1)))
                        .unwrap_or_default();
                    row(
                        ui,
                        &trf!("FC estimada a {}", "Estimated HR at {}", format::pace_unit(p)),
                        format!("{}{trend}", format::bpm(Some(h))),
                    );
                }
            });
    });
}
