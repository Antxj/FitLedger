//! Tennis area (item 20): sessions, hours, heart rate, zones and the load
//! tennis adds to the week (a hard match is not a rest day).

use super::charts::{self, Axis, ChartOpts, Series};
use super::widgets::{self, weak};
use super::{App, Page};
use crate::analysis::load::{self, day_of, week_start};
use crate::analysis::metrics;
use crate::format;
use crate::model::{Activity, Modality};
use chrono::{Datelike, Duration};
use eframe::egui::{RichText, Ui};

pub fn overview(app: &mut App, ui: &mut Ui) {
    let today = chrono::Local::now().date_naive();
    let sessions: Vec<&Activity> = app
        .data
        .activities
        .iter()
        .filter(|a| a.modality == Modality::Tennis)
        .collect();
    if sessions.is_empty() {
        ui.label(weak(
            ui,
            tr!("Nenhuma sessão de tênis ainda.", "No tennis sessions yet."),
        ));
        return;
    }
    let month_start = today.with_day(1).unwrap_or(today);
    let in_month: f64 = sessions
        .iter()
        .filter(|a| day_of(a) >= month_start)
        .filter_map(|a| a.duration())
        .sum();
    let last4: Vec<&&Activity> = sessions
        .iter()
        .filter(|a| day_of(a) > today - Duration::days(28))
        .collect();
    let hr: Vec<f64> = sessions.iter().take(10).filter_map(|a| a.avg_hr).collect();
    let tennis_load: f64 = last4
        .iter()
        .map(|a| load::activity_load(a, app.data.metrics(a.id)))
        .sum();
    let total_load: f64 = app
        .data
        .activities
        .iter()
        .filter(|a| day_of(a) > today - Duration::days(28))
        .map(|a| load::activity_load(a, app.data.metrics(a.id)))
        .sum();
    let cards = vec![
        (
            tr!("Horas no mês", "Hours this month").to_string(),
            format::hours_short(in_month),
            None,
        ),
        (
            tr!("Sessões (4 semanas)", "Sessions (4 weeks)").into(),
            last4.len().to_string(),
            Some(trf!(
                "{} por semana",
                "{} per week",
                format::num(last4.len() as f64 / 4.0, 1)
            )),
        ),
        (
            tr!("FC média recente", "Recent avg HR").into(),
            if hr.is_empty() {
                format::dash()
            } else {
                format::bpm(Some(hr.iter().sum::<f64>() / hr.len() as f64))
            },
            Some(tr!("últimas 10 sessões", "last 10 sessions").into()),
        ),
        (
            tr!("Carga cardiovascular (4 semanas)", "Cardio load (4 weeks)").into(),
            format!("{}", tennis_load.round()),
            (total_load > 0.0).then(|| {
                trf!(
                    "{}% da carga total",
                    "{}% of total load",
                    format::num(tennis_load / total_load * 100.0, 0)
                )
            }),
        ),
    ];
    widgets::card_grid(ui, 190.0, &cards);

    let ws = week_start(today);
    let weeks: Vec<_> = (0..12).rev().map(|k| ws - Duration::days(7 * k)).collect();
    ui.columns(2, |c| {
        let bars = weeks
            .iter()
            .map(|s| {
                let n = sessions
                    .iter()
                    .filter(|a| (*s..*s + Duration::days(7)).contains(&day_of(a)))
                    .count() as f64;
                (
                    charts::date_to_x(*s),
                    n,
                    widgets::modality_color(Modality::Tennis),
                    format!("{} · {n}", format::date_short(*s)),
                )
            })
            .collect();
        let mut o = ChartOpts::new("tn-weeks", Axis::Date, Axis::Plain);
        o.title = Some(tr!("Sessões por semana", "Sessions per week"));
        o.height = 150.0;
        charts::bar_chart(&mut c[0], o, bars, 5.0);

        let months: Vec<_> = (0..12)
            .rev()
            .filter_map(|k| month_start.checked_sub_months(chrono::Months::new(k)))
            .collect();
        let bars = months
            .iter()
            .map(|m| {
                let next = m.checked_add_months(chrono::Months::new(1)).unwrap_or(*m);
                let h: f64 = sessions
                    .iter()
                    .filter(|a| (*m..next).contains(&day_of(a)))
                    .filter_map(|a| a.duration())
                    .sum::<f64>()
                    / 3600.0;
                (
                    charts::date_to_x(*m),
                    h,
                    widgets::modality_color(Modality::Tennis),
                    format!(
                        "{} {} · {}",
                        format::month_name(m.month()),
                        m.year(),
                        format::hours_short(h * 3600.0)
                    ),
                )
            })
            .collect();
        let mut o = ChartOpts::new("tn-months", Axis::Date, Axis::Hours);
        o.title = Some(tr!("Horas por mês", "Hours per month"));
        o.height = 150.0;
        charts::bar_chart(&mut c[1], o, bars, 20.0);
    });

    // weekly load: tennis vs everything else
    let daily_all = load::daily_load(&app.data.activities, &app.data.metrics);
    let tennis_only: Vec<Activity> = sessions.iter().map(|a| (*a).clone()).collect();
    let daily_tn = load::daily_load(&tennis_only, &app.data.metrics);
    let mut tn = Vec::new();
    let mut other = Vec::new();
    for s in &weeks {
        let t: f64 = daily_tn.range(*s..*s + Duration::days(7)).map(|(_, v)| v).sum();
        let all: f64 = daily_all.range(*s..*s + Duration::days(7)).map(|(_, v)| v).sum();
        tn.push([charts::date_to_x(*s), t]);
        other.push([charts::date_to_x(*s), all]);
    }
    let mut o = ChartOpts::new("tn-load", Axis::Date, Axis::Plain);
    o.title = Some(tr!("Carga semanal: tênis e total", "Weekly load: tennis and total"));
    o.height = 150.0;
    o.legend = true;
    charts::line_chart(
        ui,
        o,
        vec![
            Series::line(tr!("total", "total"), widgets::ACCENT, other),
            Series::line(Modality::Tennis.label(), widgets::modality_color(Modality::Tennis), tn),
        ],
    );

    // zones of the last 30 days and the latest session's HR
    let mut zones = [0.0; 6];
    for a in sessions.iter().filter(|a| day_of(a) > today - Duration::days(30)) {
        if let Some(m) = app.data.metrics(a.id) {
            for (z, v) in m.zones_s.iter().enumerate() {
                zones[z] += v;
            }
        }
    }
    let latest = sessions[0].clone();
    let latest_m = app.data.metrics(latest.id).cloned();
    let mut open = false;
    ui.columns(2, |c| {
        let ui = &mut c[0];
        ui.label(RichText::new(tr!("Tempo por zona (30 dias)", "Time per zone (30 days)")).strong());
        let m = metrics::Metrics {
            zones_s: zones,
            zone_lower: latest_m.as_ref().map(|m| m.zone_lower).unwrap_or_default(),
            zone_max: latest_m.as_ref().map(|m| m.zone_max).unwrap_or(190.0),
            ..Default::default()
        };
        super::activity::zones_table(ui, &m);
        let ui = &mut c[1];
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(trf!(
                    "FC na última sessão ({})",
                    "HR in the latest session ({})",
                    format::date(day_of(&latest))
                ))
                .strong(),
            );
            open = ui.link(tr!("abrir", "open")).clicked();
        });
        if let Ok(st) = metrics::stream_for(&app.db, &latest) {
            let pts: Vec<[f64; 2]> = (0..st.len()).filter_map(|i| Some([st.t[i], st.hr[i]?])).collect();
            let mut o = ChartOpts::new("tn-hr", Axis::Time, Axis::Bpm);
            o.height = 170.0;
            charts::line_chart(ui, o, vec![Series::line(tr!("FC", "HR"), widgets::HR, pts)]);
        }
    });
    ui.add_space(6.0);
    if open {
        app.go(Page::Activity(latest.id));
    }
}
