//! Main dashboard (item 3): latest activity, this week, indicators, load,
//! recovery, goals in progress and upcoming planned workouts.

use super::charts::{self, Axis, ChartOpts};
use super::widgets::{self, weak};
use super::{App, Page};
use crate::analysis::load::{self, day_of, week_start};
use crate::analysis::splits::RECORD_DISTANCES;
use crate::format;
use chrono::{Datelike, Duration};
use eframe::egui::{self, RichText, Ui};

pub fn show(app: &mut App, ui: &mut Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| content(app, ui));
}

fn content(app: &mut App, ui: &mut Ui) {
    widgets::page_title(ui, "Dashboard");
    let today = chrono::Local::now().date_naive();
    let acts = &app.data.activities;
    if acts.is_empty() {
        widgets::empty_state(
            ui,
            tr!(
                "Nenhuma atividade ainda. Coloque seus arquivos .fit na pasta /fits (ao lado do FitLedger.exe) ou use \"Importar arquivos FIT\".",
                "No activities yet. Put your .fit files in the /fits folder (next to FitLedger.exe) or use \"Import FIT files\"."
            ),
        );
        ui.vertical_centered(|ui| {
            if ui.button(tr!("🔄 Procurar em /fits", "🔄 Look in /fits")).clicked() {
                app.import_from_folder(false);
            }
        });
        return;
    }
    let month_start = today.with_day(1).unwrap_or(today);
    let in_month: Vec<_> = acts.iter().filter(|a| day_of(a) >= month_start).collect();
    let run_km_month: f64 = in_month
        .iter()
        .filter(|a| a.modality.is_run())
        .filter_map(|a| a.distance())
        .sum::<f64>()
        / 1000.0;
    let hours_month: f64 = in_month.iter().filter_map(|a| a.duration()).sum::<f64>() / 3600.0;
    let longest = acts
        .iter()
        .filter(|a| a.modality.is_run())
        .max_by(|a, b| a.distance().unwrap_or(0.0).total_cmp(&b.distance().unwrap_or(0.0)));
    let best_pace = acts
        .iter()
        .filter(|a| a.modality.is_run() && a.distance().unwrap_or(0.0) >= 3000.0)
        .filter_map(|a| a.pace_s_per_km())
        .fold(f64::MAX, f64::min);
    let best5k = best_effort(app, 5000.0);
    let recent_hr: Vec<f64> = acts
        .iter()
        .filter(|a| a.modality.is_run() && (today - day_of(a)).num_days() < 30)
        .filter_map(|a| a.avg_hr)
        .collect();
    let weeks_active = (0..8)
        .filter(|k| {
            let ws = week_start(today) - Duration::days(7 * k);
            acts.iter().any(|a| (ws..ws + Duration::days(7)).contains(&day_of(a)))
        })
        .count();

    let cards = vec![
        (
            tr!("Km corridos no mês", "Km run this month").to_string(),
            format::num(run_km_month, 1),
            Some(format::month_name(today.month()).to_string()),
        ),
        (
            tr!("Horas treinadas no mês", "Hours trained this month").into(),
            format::hours_short(hours_month * 3600.0),
            None,
        ),
        (
            tr!("Atividades", "Activities").into(),
            acts.len().to_string(),
            Some(trf!("{} neste mês", "{} this month", in_month.len())),
        ),
        (
            tr!("Maior corrida", "Longest run").into(),
            format::opt_km(longest.and_then(|a| a.distance())),
            longest.map(|a| format::date(day_of(a))),
        ),
        (
            tr!("Melhor 5 km", "Best 5 km").into(),
            best5k.map(|(t, _)| format::duration(t)).unwrap_or_else(format::dash),
            best5k.map(|(t, _)| format::pace_unit(t / 5.0)),
        ),
        (
            tr!("Melhor pace (≥ 3 km)", "Best pace (≥ 3 km)").into(),
            if best_pace < f64::MAX {
                format::pace_unit(best_pace)
            } else {
                format::dash()
            },
            None,
        ),
        (
            tr!("FC média recente", "Recent avg HR").into(),
            if recent_hr.is_empty() {
                format::dash()
            } else {
                format::bpm(Some(recent_hr.iter().sum::<f64>() / recent_hr.len() as f64))
            },
            Some(tr!("corridas, 30 dias", "runs, 30 days").into()),
        ),
        (
            tr!("Consistência semanal", "Weekly consistency").into(),
            format!("{weeks_active}/8"),
            Some(tr!("semanas com treino", "weeks with training").into()),
        ),
    ];
    widgets::card_grid(ui, 170.0, &cards);

    ui.add_space(8.0);
    ui.columns(2, |cols| {
        latest(app, &mut cols[0]);
        this_week(app, &mut cols[1], today);
    });

    ui.add_space(8.0);
    ui.columns(2, |cols| {
        load_box(app, &mut cols[0], today);
        goals_box(app, &mut cols[1], today);
    });
    ui.add_space(8.0);
    upcoming(app, ui, today);
}

pub fn best_effort(app: &App, dist: f64) -> Option<(f64, i64)> {
    debug_assert!(RECORD_DISTANCES.contains(&dist) || dist > 0.0);
    app.db
        .conn
        .query_row(
            "SELECT time_s, activity_id FROM best_effort WHERE abs(distance_m - ?1) < 1 ORDER BY time_s LIMIT 1",
            [dist],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()
}

fn latest(app: &mut App, ui: &mut Ui) {
    let Some(a) = app.data.activities.first().cloned() else {
        return;
    };
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(weak(ui, tr!("Treino mais recente", "Latest activity")).small());
        ui.horizontal(|ui| {
            ui.label(RichText::new(a.title()).size(18.0).strong());
            widgets::modality_chip(ui, a.modality);
        });
        ui.label(weak(ui, format::datetime(a.local_start())));
        let mut parts = vec![format::opt_duration(a.duration())];
        if a.distance().is_some() {
            parts.push(format::opt_km(a.distance()));
        }
        if a.modality.is_run() {
            parts.push(format::opt_pace(a.pace_s_per_km()));
        }
        if a.avg_hr.is_some() {
            parts.push(format::bpm(a.avg_hr));
        }
        ui.label(RichText::new(parts.join("  ·  ")).size(15.0));
        if ui.button(tr!("Abrir", "Open")).clicked() {
            app.go(Page::Activity(a.id));
        }
    });
}

fn this_week(app: &mut App, ui: &mut Ui, today: chrono::NaiveDate) {
    let ws = week_start(today);
    let s = load::week_summary(&app.data.activities, &app.data.metrics, ws);
    let prev = load::week_summary(&app.data.activities, &app.data.metrics, ws - Duration::days(7));
    let prev4: f64 = (1..=4)
        .map(|k| load::week_summary(&app.data.activities, &app.data.metrics, ws - Duration::days(7 * k)).load)
        .sum::<f64>()
        / 4.0;
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(weak(ui, tr!("Semana atual (resumo)", "This week (summary)")).small());
        egui::Grid::new("week")
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
                    tr!("Corrida", "Running"),
                    format!("{} km ({}×)", format::num(s.run_km, 1), s.run_count),
                );
                row(ui, tr!("Tênis", "Tennis"), format::hours_short(s.tennis_s));
                row(
                    ui,
                    tr!("Musculação", "Strength"),
                    trf!("{} sessões", "{} sessions", s.strength_sessions),
                );
                row(ui, tr!("Tempo total", "Total time"), format::hours_short(s.total_s));
                row(
                    ui,
                    tr!("Corrida longa", "Long run"),
                    format!("{} km", format::num(s.longest_run_km, 1)),
                );
                row(
                    ui,
                    tr!("FC média em corrida fácil", "Avg HR on easy runs"),
                    format::bpm(s.easy_hr),
                );
                row(
                    ui,
                    tr!("Carga", "Load"),
                    format!("{} ({})", s.load.round(), load::load_class(s.load, prev4)),
                );
                let vol = |w: &load::WeekSummary| w.total_s;
                let cmp = if vol(&prev) > 0.0 {
                    format::pct_signed((vol(&s) - vol(&prev)) / vol(&prev) * 100.0)
                } else {
                    format::dash()
                };
                row(ui, tr!("Vs. semana anterior (volume)", "Vs. last week (volume)"), cmp);
            });
    });
}

fn load_box(app: &mut App, ui: &mut Ui, today: chrono::NaiveDate) {
    let daily = load::daily_load(&app.data.activities, &app.data.metrics);
    let st = load::status(&daily, today);
    let (rec, why) = load::recovery(&app.data.activities, &app.data.metrics, today);
    let warns = load::warnings(&app.data.activities, &app.data.metrics, today);
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(weak(ui, tr!("Carga de treino", "Training load")).small());
        ui.horizontal(|ui| {
            ui.label(RichText::new(trf!("7 dias: {}", "7 days: {}", st.acute7.round())).strong());
            ui.label(weak(ui, trf!("· 28 dias: {}", "· 28 days: {}", st.chronic28.round())));
            ui.label(weak(ui, trf!("· semana: {}", "· week: {}", st.this_week.round())));
        });
        let bars: Vec<_> = (0..28)
            .rev()
            .map(|k| {
                let d = today - Duration::days(k);
                let v = daily.get(&d).copied().unwrap_or(0.0);
                (
                    charts::date_to_x(d),
                    v,
                    widgets::ACCENT,
                    format!("{} · {}", format::date_short(d), v.round()),
                )
            })
            .collect();
        let mut o = ChartOpts::new("dash-load", Axis::Date, Axis::Plain);
        o.height = 110.0;
        charts::bar_chart(ui, o, bars, 0.8);
        let color = match rec {
            load::Recovery::Recovered => widgets::GOOD,
            load::Recovery::Moderate => widgets::WARN,
            load::Recovery::High => widgets::BAD,
        };
        ui.horizontal(|ui| {
            ui.label(tr!("Recuperação:", "Recovery:"));
            widgets::chip(ui, rec.label(), color);
        });
        for w in &why {
            ui.label(weak(ui, format!("• {w}")).small());
        }
        ui.label(
            weak(
                ui,
                tr!(
                    "Estimativa baseada apenas nos treinos registrados. Não é avaliação médica.",
                    "Estimate based only on the recorded workouts. Not a medical assessment."
                ),
            )
            .small()
            .italics(),
        );
        for w in warns {
            widgets::note(ui, &format!("⚠ {w}"), widgets::WARN);
        }
    });
}

fn goals_box(app: &mut App, ui: &mut Ui, today: chrono::NaiveDate) {
    let goals: Vec<_> = app
        .db
        .goals()
        .unwrap_or_default()
        .into_iter()
        .filter(|g| !g.archived)
        .collect();
    let be = crate::export::best_efforts_map(&app.db);
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(weak(ui, tr!("Metas em andamento", "Goals in progress")).small());
        if goals.is_empty() {
            ui.label(weak(ui, tr!("Nenhuma meta criada.", "No goals yet.")));
        }
        for g in goals.iter().take(6) {
            let p = crate::analysis::goals::progress(g, &app.data.activities, &be, today);
            ui.label(RichText::new(&g.title).strong());
            widgets::progress(
                ui,
                p.fraction,
                &format!("{} — {}%", p.text, (p.fraction.min(1.0) * 100.0).round()),
                p.done,
            );
        }
        if ui.link(tr!("Ver metas →", "See goals →")).clicked() {
            app.go(Page::Goals);
        }
    });
}

fn upcoming(app: &mut App, ui: &mut Ui, today: chrono::NaiveDate) {
    let planned = app
        .db
        .planned_workouts(today, today + Duration::days(7))
        .unwrap_or_default();
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(weak(ui, tr!("Próximos treinos planejados", "Upcoming planned workouts")).small());
        let pending: Vec<_> = planned.iter().filter(|w| w.activity_id.is_none()).collect();
        if pending.is_empty() {
            ui.label(weak(
                ui,
                tr!(
                    "Nada planejado para os próximos 7 dias.",
                    "Nothing planned for the next 7 days."
                ),
            ));
        }
        for w in pending {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} {}",
                        format::weekday_short(w.date),
                        format::date_short(w.date)
                    ))
                    .strong(),
                );
                ui.label(RichText::new(w.display_title()).color(widgets::modality_color(w.modality)));
                let mut extra = Vec::new();
                if let Some(d) = w.distance_m {
                    extra.push(format::km(d));
                }
                if let Some(t) = w.duration_s {
                    extra.push(format::duration(t));
                }
                if let Some(p) = w.target_pace {
                    extra.push(format!(
                        "{} ({} km/h)",
                        format::pace_unit(p),
                        format::num(format::pace_to_kmh(p), 1)
                    ));
                }
                ui.label(weak(ui, extra.join(" · ")));
            });
        }
        if ui.link(tr!("Abrir calendário →", "Open calendar →")).clicked() {
            app.go(Page::Calendar);
        }
    });
}
