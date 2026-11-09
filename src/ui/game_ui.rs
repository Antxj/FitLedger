//! Feedback UI: stars, the weekly streak, the post-run report card, the
//! celebration animation and the Achievements page.

use super::widgets::{self, weak};
use super::{App, Page};
use crate::coach::game::{self, Achievement, Score};
use crate::format;
use eframe::egui::{self, Color32, RichText, Ui};
use std::time::{Duration, Instant};

pub const GOLD: Color32 = Color32::from_rgb(0xf5, 0xb7, 0x01);

/// "★★☆"
pub fn stars_text(n: u8) -> String {
    (0..3).map(|i| if i < n { '★' } else { '☆' }).collect()
}

pub fn stars(ui: &mut Ui, n: u8, size: f32) {
    ui.label(RichText::new(stars_text(n)).size(size).color(GOLD));
}

/// Weekly streak and the last 10 weeks as medals, in one line.
pub fn streak_card(ui: &mut Ui, g: &game::State) {
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("🔥 {}", g.streak)).size(16.0).strong().color(widgets::WARN));
            ui.label(weak(
                ui,
                trf!(
                    "semana(s) seguida(s) com treino do plano · recorde {}",
                    "week(s) in a row with a plan session · best {}",
                    g.best_streak
                ),
            ));
            widgets::help(
                ui,
                tr!(
                    "Uma semana conta quando você faz pelo menos 1 treino do plano (com 2 ou mais, a marca é dourada). Uma semana sem treino a cada 8 é perdoada; semanas marcadas como doente/viagem não contam.",
                    "A week counts when you do at least 1 plan session (2 or more make it gold). One week without a session in 8 is forgiven; weeks marked as sick/travel do not count."
                ),
            );
            ui.add_space(12.0);
            for w in g.weeks.iter().rev().take(10).collect::<Vec<_>>().into_iter().rev() {
                let (txt, color, tip) = match (w.consistent, w.joker_used, w.two) {
                    (None, _, _) => ("◌", Color32::GRAY, tr!("doente/viagem: não conta", "sick/travel: not counted")),
                    (Some(true), _, true) => ("●", GOLD, tr!("2 ou mais treinos", "2 or more sessions")),
                    (Some(true), _, _) => ("●", widgets::GOOD, tr!("treinou", "trained")),
                    (Some(false), true, _) => ("◐", widgets::WARN, tr!("perdoada", "forgiven")),
                    (Some(false), false, _) => ("○", widgets::BAD, tr!("sem treino", "no session")),
                };
                ui.label(RichText::new(txt).size(16.0).color(color)).on_hover_text(trf!(
                    "semana de {} · {} treino(s) · {tip}",
                    "week of {} · {} session(s) · {tip}",
                    format::date_short(w.start),
                    w.done
                ));
            }
        });
    });
}

/// Stars + reasons of a session's score.
pub fn score_details(ui: &mut Ui, s: &Score) {
    stars(ui, s.stars(), 22.0);
    for r in &s.reasons {
        if r.starts_with('★') {
            ui.label(RichText::new(r).color(ui.visuals().text_color()));
        } else {
            ui.label(RichText::new(format!("☆ {r}")).color(ui.visuals().weak_text_color()));
        }
    }
}

/// What the report card shows after an import.
pub struct ReportCard {
    pub activity_id: i64,
    pub score: Option<Score>,
    pub new_achievements: Vec<Achievement>,
    pub highlights: Vec<String>,
}

pub fn report_card_window(app: &mut App, ctx: &egui::Context) {
    let Some(card) = app.report_card.as_ref() else { return };
    let mut open = true;
    let mut go = None;
    let a = app.data.get(card.activity_id).cloned();
    egui::Window::new(tr!("Resultado do treino", "Session result"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            if let Some(a) = &a {
                ui.label(weak(ui, format::datetime(a.local_start())));
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
            }
            ui.add_space(6.0);
            match &card.score {
                Some(s) => {
                    if let Some(t) = app
                        .coach
                        .as_ref()
                        .and_then(|c| c.sessions.iter().find(|t| t.id == s.session_id))
                    {
                        ui.label(
                            RichText::new(trf!("Treino do plano: {}", "Plan session: {}", t.session.kind.label()))
                                .strong(),
                        );
                    }
                    score_details(ui, s);
                }
                None => {
                    ui.label(tr!(
                        "Corrida fora do plano: conta para a sua forma, mas não é avaliada contra um treino proposto.",
                        "Run outside the plan: it counts for your fitness, but it is not rated against a proposed session."
                    ));
                }
            }
            for h in &card.highlights {
                ui.label(RichText::new(format!("📈 {h}")).color(widgets::GOOD));
            }
            const SHOWN: usize = 4;
            for ach in card.new_achievements.iter().take(SHOWN) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(ach.icon).size(20.0));
                    ui.label(RichText::new(trf!("Conquista: {}", "Achievement: {}", ach.title)).strong());
                });
            }
            if card.new_achievements.len() > SHOWN {
                ui.label(weak(
                    ui,
                    trf!(
                        "+ {} outra(s) conquista(s)",
                        "+ {} more achievement(s)",
                        card.new_achievements.len() - SHOWN
                    ),
                ));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("Ver a corrida", "See the run")).clicked() {
                    go = Some(Page::Activity(card.activity_id));
                }
                if ui.button(tr!("Próximo treino", "Next session")).clicked() {
                    go = Some(Page::Today);
                }
                if !card.new_achievements.is_empty() && ui.button(tr!("Conquistas", "Achievements")).clicked() {
                    go = Some(Page::Achievements);
                }
            });
        });
    if let Some(p) = go {
        app.report_card = None;
        app.go(p);
    } else if !open {
        app.report_card = None;
    }
}

/// Short confetti burst drawn on top of everything.
pub struct Celebration {
    start: Instant,
    particles: Vec<(f32, f32, f32, f32, Color32)>,
}

const CELEBRATION: Duration = Duration::from_millis(1600);

impl Celebration {
    pub fn new(seed: u64) -> Celebration {
        let mut x = seed | 1;
        let mut rnd = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % 10_000) as f32 / 10_000.0
        };
        let colors = [GOLD, widgets::ACCENT, widgets::GOOD, widgets::HR, Color32::WHITE];
        let particles = (0..90)
            .map(|i| {
                (
                    rnd(),
                    rnd() * 0.3,
                    (rnd() - 0.5) * 0.6,
                    0.3 + rnd() * 0.5,
                    colors[i % colors.len()],
                )
            })
            .collect();
        Celebration {
            start: Instant::now(),
            particles,
        }
    }

    /// Draws a frame; returns false when finished.
    pub fn draw(&self, ctx: &egui::Context) -> bool {
        let t = self.start.elapsed().as_secs_f32();
        if t > CELEBRATION.as_secs_f32() {
            return false;
        }
        let rect = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("confetti")));
        let fade = 1.0 - (t / CELEBRATION.as_secs_f32()).powi(2);
        for &(x0, y0, vx, vy, c) in &self.particles {
            let x = rect.left() + (x0 + vx * t) * rect.width();
            let y = rect.top() + (y0 - 0.15 + vy * t + 0.35 * t * t) * rect.height();
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(6.0, 9.0)),
                1.5,
                c.gamma_multiply(fade),
            );
        }
        ctx.request_repaint();
        true
    }
}

pub fn show_page(app: &mut App, ui: &mut Ui) {
    widgets::page_title(ui, tr!("Conquistas", "Achievements"));
    let Some(g) = app.game.clone() else {
        widgets::empty_state(
            ui,
            tr!(
                "A avaliação dos treinos está desligada em Configurações.",
                "Session feedback is turned off in Settings."
            ),
        );
        return;
    };
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        streak_card(ui, &g);
        egui::CollapsingHeader::new(tr!("Como cada treino é avaliado", "How each session is rated")).show(ui, |ui| {
            for l in [
                tr!(
                    "★ Feito: o treino do plano no tamanho proposto (90% a 130% do tempo).",
                    "★ Done: the plan session at the proposed size (90% to 130% of the time)."
                ),
                tr!(
                    "★ Esforço: FC dentro do limite nas corridas leves (no máximo 10% do tempo acima); pace no intervalo nos treinos fortes.",
                    "★ Effort: HR within the cap on easy runs (at most 10% of the time above); pace in range on hard sessions."
                ),
                tr!(
                    "★ Objetivo: sem exagerar nas leves; blocos completos; teste completo; meta atingida na tentativa.",
                    "★ Target: no overdoing on easy runs; all blocks; complete check; goal reached on the attempt."
                ),
                tr!(
                    "Correr mais ou mais forte que o proposto não melhora a avaliação.",
                    "Running more or harder than proposed does not improve the rating."
                ),
            ] {
                ui.label(l);
            }
        });
        widgets::section(ui, tr!("Troféus", "Trophies"));
        let unlocked = g.achievements.iter().filter(|a| a.unlocked.is_some()).count();
        ui.label(weak(
            ui,
            trf!(
                "{} de {} desbloqueados",
                "{} of {} unlocked",
                unlocked,
                g.achievements.len()
            ),
        ));
        let w = ui.available_width();
        let per_row = ((w + 8.0) / 228.0).floor().max(1.0) as usize;
        let card_w = (w - 8.0 * (per_row as f32 - 1.0)) / per_row as f32;
        let mut list: Vec<&Achievement> = g.achievements.iter().collect();
        list.sort_by_key(|a| (a.unlocked.is_none(), std::cmp::Reverse(a.unlocked)));
        for row in list.chunks(per_row) {
            ui.horizontal(|ui| {
                for a in row {
                    let on = a.unlocked.is_some();
                    widgets::card_frame(ui).show(ui, |ui| {
                        ui.set_width(card_w - 30.0);
                        ui.set_min_height(86.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let icon = RichText::new(a.icon).size(24.0);
                                ui.label(if on { icon } else { icon.color(Color32::from_gray(110)) });
                                ui.label(if on {
                                    RichText::new(&a.title).strong()
                                } else {
                                    weak(ui, &a.title)
                                });
                            });
                            ui.label(weak(ui, &a.how).small());
                            match (a.unlocked, a.progress) {
                                (Some(d), _) => {
                                    ui.label(
                                        RichText::new(trf!("✔ {}", "✔ {}", format::date(d)))
                                            .small()
                                            .color(widgets::GOOD),
                                    );
                                }
                                (None, Some((x, t))) => {
                                    widgets::progress(ui, x as f32 / t.max(1) as f32, &format!("{x}/{t}"), false);
                                }
                                _ => {
                                    ui.label(weak(ui, "🔒").small());
                                }
                            }
                        });
                    });
                }
            });
            ui.add_space(4.0);
        }
    });
}

/// The way to the goal as a trail: distance milestones (or, once the
/// distance is there, heart-rate milestones), reached with the date, and the
/// goal at the end, with "you are here".
pub fn journey(ui: &mut Ui, c: &crate::coach::store::CoachState, runs: &[crate::model::Activity]) {
    use crate::coach::fitness::day;
    let goal_km = c.goal.distance_m / 1000.0;
    let since: Vec<&crate::model::Activity> = runs.iter().filter(|a| day(a) >= c.goal.start_date).collect();
    let before = runs
        .iter()
        .filter(|a| day(a) < c.goal.start_date)
        .filter_map(|a| a.distance())
        .fold(0.0, f64::max)
        / 1000.0;
    let step = if goal_km <= 4.0 { 0.5 } else { 1.0 };
    let mut marks: Vec<f64> = Vec::new();
    let mut k = ((before / step).floor() + 1.0) * step;
    while k < goal_km - 0.01 {
        marks.push(k);
        k += step;
    }
    // at most 7 intermediate milestones
    while marks.len() > 7 {
        marks = marks
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, v)| v)
            .collect();
    }
    let reached = |km: f64| -> Option<chrono::NaiveDate> {
        let mut v: Vec<&&crate::model::Activity> = since
            .iter()
            .filter(|a| a.distance().unwrap_or(0.0) >= km * 980.0)
            .collect();
        v.sort_by_key(|a| a.start_time);
        v.first().map(|a| day(a))
    };
    let mut nodes: Vec<(String, Option<chrono::NaiveDate>, bool)> =
        vec![(tr!("Início", "Start").into(), Some(c.goal.start_date), true)];
    // the distance is already there and the goal is about heart rate: the
    // milestones are the HR at goal pace coming down
    let hr = &c.fitness.hr_history;
    let hr_start = hr.iter().find(|x| x.0 >= c.goal.start_date).or(hr.first()).map(|x| x.1);
    match (c.goal.target_hr, hr_start) {
        (Some(target), Some(from)) if marks.iter().all(|m| reached(*m).is_some()) && from > target + 2.0 => {
            let step = if from - target <= 15.0 { 3.0 } else { 5.0 };
            let mut m = (from / step).floor() * step;
            while m > target + 1.0 {
                // a run below the milestone, or today's estimate (all recent runs) below it
                let r = hr
                    .iter()
                    .find(|x| x.0 >= c.goal.start_date && x.1 <= m)
                    .map(|x| x.0)
                    .or_else(|| {
                        c.fitness
                            .hr_at_goal_pace
                            .filter(|h| *h <= m)
                            .and(hr.last().map(|x| x.0))
                    });
                nodes.push((format!("{} bpm", m.round()), r, r.is_some()));
                m -= step;
            }
            nodes.truncate(8);
        }
        _ => {
            for m in &marks {
                let r = reached(*m);
                nodes.push((format!("{} km", format::num_trim(*m, 1)), r, r.is_some()));
            }
        }
    }
    let goal_done = c.reached.map(|r| r.0);
    nodes.push((tr!("🏆 Meta", "🏆 Goal").into(), goal_done, goal_done.is_some()));
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(weak(ui, tr!("Jornada até a meta", "Journey to the goal")).small());
            if c.reached.is_none() {
                ui.label(
                    weak(
                        ui,
                        trf!(
                            "· chegada estimada {}",
                            "· estimated arrival {}",
                            format::date(c.projection.date)
                        ),
                    )
                    .small(),
                );
            }
        });
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 86.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let n = nodes.len();
        let x = |i: usize| rect.left() + 24.0 + (rect.width() - 48.0) * i as f32 / (n - 1).max(1) as f32;
        let y = rect.top() + 34.0;
        let last_done = nodes.iter().rposition(|n| n.2).unwrap_or(0);
        painter.line_segment(
            [egui::pos2(x(0), y), egui::pos2(x(n - 1), y)],
            egui::Stroke::new(4.0, Color32::from_gray(90)),
        );
        painter.line_segment(
            [egui::pos2(x(0), y), egui::pos2(x(last_done), y)],
            egui::Stroke::new(4.0, widgets::GOOD),
        );
        for (i, (label, date, done)) in nodes.iter().enumerate() {
            let p = egui::pos2(x(i), y);
            if *done {
                painter.circle_filled(p, 9.0, if i == n - 1 { GOLD } else { widgets::GOOD });
            } else {
                painter.circle_filled(p, 9.0, ui.visuals().extreme_bg_color);
                painter.circle_stroke(p, 9.0, egui::Stroke::new(2.0, Color32::from_gray(120)));
            }
            painter.text(
                p + egui::vec2(0.0, 16.0),
                egui::Align2::CENTER_TOP,
                label,
                egui::FontId::proportional(12.0),
                ui.visuals().text_color(),
            );
            if let (Some(d), true) = (date, *done) {
                painter.text(
                    p + egui::vec2(0.0, 32.0),
                    egui::Align2::CENTER_TOP,
                    format::date_short(*d),
                    egui::FontId::proportional(10.5),
                    ui.visuals().weak_text_color(),
                );
            }
        }
        if c.reached.is_none() && last_done + 1 < n {
            // "you are here", between the last reached and the next milestone
            let p = egui::pos2((x(last_done) + x(last_done + 1)) / 2.0, y - 14.0);
            painter.text(
                p,
                egui::Align2::CENTER_BOTTOM,
                tr!("▼ você está aqui", "▼ you are here"),
                egui::FontId::proportional(11.0),
                widgets::ACCENT,
            );
        }
        if let (Some(target), Some(now)) = (c.goal.target_hr, c.fitness.hr_at_goal_pace) {
            ui.label(weak(
                ui,
                trf!(
                    "FC no pace da meta: {} → meta ≤ {} bpm",
                    "HR at goal pace: {} → goal ≤ {} bpm",
                    format::bpm(Some(now)),
                    target.round()
                ),
            ));
        }
    });
}
