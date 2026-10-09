//! Progress page (item 24): totals per day/week/month for a chosen period.

use super::App;
use super::activities::Period;
use super::charts::{self, Axis, ChartOpts, Series};
use super::widgets::{self, weak};
use crate::analysis::load::day_of;
use crate::analysis::trends::{self, Bucket, Totals};
use crate::format;
use crate::model::Modality;
use chrono::{Duration, NaiveDate};
use eframe::egui::{self, Color32, Ui};

#[derive(Default)]
pub struct State {
    period: Period,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    widgets::page_title(ui, tr!("Evolução", "Progress"));
    let today = chrono::Local::now().date_naive();
    ui.horizontal(|ui| {
        let p = &mut app.pages.evolution.period;
        for (v, label) in [
            (Period::Days7, tr!("7 dias", "7 days")),
            (Period::Days30, tr!("30 dias", "30 days")),
            (Period::Months3, tr!("3 meses", "3 months")),
            (Period::Months6, tr!("6 meses", "6 months")),
            (Period::Year, tr!("1 ano", "1 year")),
            (Period::All, tr!("Tudo", "All")),
        ] {
            ui.selectable_value(p, v, label);
        }
    });
    let period = app.pages.evolution.period;
    let first = app.data.activities.iter().map(day_of).min();
    let from: Option<NaiveDate> = match period.days() {
        Some(d) => Some(today - Duration::days(d - 1)),
        None => first,
    };
    let Some(from) = from else {
        widgets::empty_state(ui, tr!("Nenhuma atividade ainda.", "No activities yet."));
        return;
    };
    let span = (today - from).num_days();
    let bucket = if span <= 31 {
        Bucket::Day
    } else if span <= 400 {
        Bucket::Week
    } else {
        Bucket::Month
    };
    let vols = app.db.strength_volumes().unwrap_or_default();
    let b = trends::buckets(
        &app.data.activities,
        &app.data.metrics,
        &vols,
        Some(from),
        today,
        bucket,
    );
    let unit = match bucket {
        Bucket::Day => tr!("por dia", "per day"),
        Bucket::Week => tr!("por semana", "per week"),
        Bucket::Month => tr!("por mês", "per month"),
    };
    let width = match bucket {
        Bucket::Day => 0.8,
        Bucket::Week => 5.5,
        Bucket::Month => 24.0,
    };
    let totals: Totals = b.values().fold(Totals::default(), |mut acc, t| {
        acc.run_km += t.run_km;
        acc.hours += t.hours;
        acc.sessions += t.sessions;
        acc.tennis_h += t.tennis_h;
        acc.strength_sessions += t.strength_sessions;
        acc.load += t.load;
        acc.run_m += t.run_m;
        acc.run_s += t.run_s;
        acc
    });
    widgets::card_grid(
        ui,
        150.0,
        &[
            (
                tr!("Corrida", "Running").into(),
                format!("{} km", format::num(totals.run_km, 1)),
                None,
            ),
            (
                tr!("Horas", "Hours").into(),
                format::hours_short(totals.hours * 3600.0),
                None,
            ),
            (
                tr!("Atividades", "Activities").into(),
                format!("{}", totals.sessions),
                None,
            ),
            (
                tr!("Pace médio", "Avg pace").into(),
                format::opt_pace(totals.avg_pace()),
                None,
            ),
            (
                tr!("Tênis", "Tennis").into(),
                format::hours_short(totals.tennis_h * 3600.0),
                None,
            ),
            (
                tr!("Musculação", "Strength").into(),
                trf!("{} sessões", "{} sessions", totals.strength_sessions),
                None,
            ),
            (tr!("Carga", "Load").into(), format::int(totals.load), None),
        ],
    );
    let x = |d: &NaiveDate| charts::date_to_x(*d);
    let bars = |f: &dyn Fn(&Totals) -> f64, color: Color32, y: Axis| -> Vec<(f64, f64, Color32, String)> {
        b.iter()
            .map(|(d, t)| (x(d), f(t), color, format!("{} · {}", format::date(*d), y.fmt(f(t)))))
            .collect()
    };
    let line = |f: &dyn Fn(&Totals) -> Option<f64>| -> Vec<[f64; 2]> {
        b.iter().filter_map(|(d, t)| Some([x(d), f(t)?])).collect()
    };
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let chart = |ui: &mut Ui, id: &'static str, title: String, y: Axis, data: Vec<(f64, f64, Color32, String)>| {
            let mut o = ChartOpts::new(id, Axis::Date, y);
            o.title = Some(title.as_str());
            o.height = 170.0;
            o.link = Some("evo");
            charts::bar_chart(ui, o, data, width);
        };
        ui.columns(2, |c| {
            chart(
                &mut c[0],
                "evo-km",
                trf!("Km {}", "Km {}", unit),
                Axis::Plain,
                bars(&|t| t.run_km, widgets::modality_color(Modality::RoadRun), Axis::Plain),
            );
            chart(
                &mut c[1],
                "evo-h",
                trf!("Horas de treino {}", "Training hours {}", unit),
                Axis::Hours,
                bars(&|t| t.hours, widgets::ACCENT, Axis::Hours),
            );
        });
        ui.columns(2, |c| {
            let pace = line(&|t| t.avg_pace());
            let mut o = ChartOpts::new("evo-pace", Axis::Date, Axis::Pace);
            o.title = Some(tr!("Pace médio (corridas)", "Average pace (runs)"));
            o.height = 170.0;
            o.invert_y = true;
            o.link = Some("evo");
            charts::line_chart(
                &mut c[0],
                o,
                vec![
                    Series::line("Pace", widgets::PACE, pace.clone()),
                    Series::dots("Pace", widgets::PACE, pace),
                ],
            );
            let hr = line(&|t| t.avg_hr());
            let mut o = ChartOpts::new("evo-hr", Axis::Date, Axis::Bpm);
            o.title = Some(tr!("FC média (corridas)", "Average HR (runs)"));
            o.height = 170.0;
            o.link = Some("evo");
            charts::line_chart(
                &mut c[1],
                o,
                vec![
                    Series::line(tr!("FC", "HR"), widgets::HR, hr.clone()),
                    Series::dots(tr!("FC", "HR"), widgets::HR, hr),
                ],
            );
        });
        ui.columns(2, |c| {
            // HR at the most common pace
            let runs: Vec<_> = app
                .data
                .activities
                .iter()
                .filter(|a| a.modality.is_run() && day_of(a) >= from)
                .collect();
            let bands = trends::pace_bands(runs.iter().copied(), &app.data.metrics);
            let ui0 = &mut c[0];
            if let Some(p) = trends::typical_pace(&bands) {
                let pts: Vec<[f64; 2]> = trends::hr_at_pace(runs.iter().copied(), &app.data.metrics, p)
                    .iter()
                    .map(|h| [charts::date_to_x(h.date), h.hr])
                    .collect();
                let title = trf!("FC no pace de {}/km", "HR at {}/km", format::pace(trends::pace_band(p)));
                let mut o = ChartOpts::new("evo-hrpace", Axis::Date, Axis::Bpm);
                o.title = Some(title.as_str());
                o.height = 170.0;
                o.link = Some("evo");
                charts::line_chart(
                    ui0,
                    o,
                    vec![
                        Series::line(tr!("FC", "HR"), widgets::HR, pts.clone()),
                        Series::dots(tr!("FC", "HR"), widgets::HR, pts),
                    ],
                );
            } else {
                ui0.label(weak(
                    ui0,
                    tr!(
                        "FC por pace: ainda sem trechos estáveis.",
                        "HR by pace: no steady stretches yet."
                    ),
                ));
            }
            chart(
                &mut c[1],
                "evo-long",
                tr!("Maior corrida (km)", "Longest run (km)").into(),
                Axis::Plain,
                bars(
                    &|t| t.longest_km,
                    widgets::modality_color(Modality::Treadmill),
                    Axis::Plain,
                ),
            );
        });
        ui.columns(2, |c| {
            chart(
                &mut c[0],
                "evo-vol",
                tr!("Volume de musculação (kg)", "Strength volume (kg)").into(),
                Axis::Plain,
                bars(
                    &|t| t.strength_volume,
                    widgets::modality_color(Modality::Strength),
                    Axis::Plain,
                ),
            );
            chart(
                &mut c[1],
                "evo-tennis",
                tr!("Horas de tênis", "Tennis hours").into(),
                Axis::Hours,
                bars(&|t| t.tennis_h, widgets::modality_color(Modality::Tennis), Axis::Hours),
            );
        });
        chart(
            ui,
            "evo-load",
            trf!("Carga {}", "Load {}", unit),
            Axis::Plain,
            bars(&|t| t.load, widgets::WARN, Axis::Plain),
        );
    });
}
