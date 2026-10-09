//! Comparison of two or more activities (items 23 and 39): side-by-side
//! table, overlaid charts by distance or time, and comparability warnings.

use super::charts::{self, Axis, ChartOpts, Series};
use super::widgets::{self, weak};
use super::{App, Page};
use crate::analysis::load::day_of;
use crate::analysis::metrics::{self, Terrain};
use crate::analysis::stream::Stream;
use crate::analysis::trends;
use crate::format;
use crate::model::Activity;
use eframe::egui::{self, RichText, Ui};
use std::collections::HashMap;

#[derive(Default)]
pub struct State {
    key: Vec<i64>,
    generation: u64,
    streams: HashMap<i64, Stream>,
    by_distance: bool,
}

type RowFn = Box<dyn Fn(&Activity) -> String>;

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if ui.button(tr!("← Voltar", "← Back")).clicked() {
            app.back();
        }
        widgets::page_title(ui, tr!("Comparar atividades", "Compare activities"));
    });
    let mut acts: Vec<Activity> = app
        .compare_ids
        .iter()
        .filter_map(|id| app.data.get(*id).cloned())
        .collect();
    acts.sort_by_key(|a| a.start_time);
    if acts.len() < 2 {
        widgets::empty_state(
            ui,
            tr!(
                "Marque duas ou mais atividades na lista de atividades e clique em Comparar.",
                "Tick two or more activities in the activity list and click Compare."
            ),
        );
        return;
    }
    let ids: Vec<i64> = acts.iter().map(|a| a.id).collect();
    if app.pages.compare.key != ids || app.pages.compare.generation != app.data.generation {
        let mut streams = HashMap::new();
        for a in &acts {
            streams.insert(a.id, metrics::stream_for(&app.db, a).unwrap_or_default());
        }
        let st = &mut app.pages.compare;
        st.streams = streams;
        st.key = ids;
        st.generation = app.data.generation;
        st.by_distance = acts.iter().all(|a| a.distance().is_some());
    }
    let refs: Vec<&Activity> = acts.iter().collect();
    for n in trends::comparability_notes(&refs, &app.data.metrics) {
        widgets::note(ui, &format!("⚠ {n}"), widgets::WARN);
    }
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        table(app, ui, &acts);
        ui.add_space(8.0);
        overlay(app, ui, &acts);
    });
}

fn table(app: &mut App, ui: &mut Ui, acts: &[Activity]) {
    let mut open = None;
    egui::Grid::new("cmp")
        .striped(true)
        .min_col_width(120.0)
        .show(ui, |ui| {
            ui.label("");
            for (i, a) in acts.iter().enumerate() {
                ui.vertical(|ui| {
                    if ui
                        .link(
                            RichText::new(format::date(day_of(a)))
                                .strong()
                                .color(widgets::SERIES[i % 6]),
                        )
                        .clicked()
                    {
                        open = Some(a.id);
                    }
                    ui.label(weak(ui, a.modality.label()));
                });
            }
            ui.end_row();
            let rows: Vec<(&str, RowFn)> = vec![
                (
                    tr!("Distância", "Distance"),
                    Box::new(|a: &Activity| format::opt_km(a.distance())),
                ),
                (
                    tr!("Tempo", "Time"),
                    Box::new(|a: &Activity| format::opt_duration(a.duration())),
                ),
                ("Pace", Box::new(|a: &Activity| format::opt_pace(a.pace_s_per_km()))),
                (
                    tr!("FC média", "Avg HR"),
                    Box::new(|a: &Activity| format::bpm(a.avg_hr)),
                ),
                (
                    tr!("FC máxima", "Max HR"),
                    Box::new(|a: &Activity| format::bpm(a.max_hr)),
                ),
                (
                    tr!("Cadência", "Cadence"),
                    Box::new(|a: &Activity| {
                        a.avg_cadence
                            .map(|c| format!("{} spm", c.round()))
                            .unwrap_or_else(format::dash)
                    }),
                ),
            ];
            for (label, f) in rows {
                ui.label(weak(ui, label));
                for a in acts {
                    ui.label(f(a));
                }
                ui.end_row();
            }
            ui.label(weak(ui, tr!("Elevação", "Elevation")));
            for a in acts {
                let m = app.data.metrics(a.id);
                let g = a.ascent_m.or(m.and_then(|m| m.gain_m));
                ui.label(format!(
                    "{} ({})",
                    g.map(|g| format!("+{} m", g.round())).unwrap_or_else(format::dash),
                    Terrain::of(a, m).label()
                ));
            }
            ui.end_row();
            ui.label(weak(ui, tr!("Deriva cardíaca", "Cardiac drift")));
            for a in acts {
                let d = app
                    .data
                    .metrics(a.id)
                    .and_then(|m| m.drift.as_ref())
                    .and_then(|d| d.decoupling_pct);
                ui.label(
                    d.map(|v| format!("{}%", format::num(v, 1)))
                        .unwrap_or_else(format::dash),
                );
            }
            ui.end_row();
            ui.label(weak(ui, tr!("Temperatura", "Temperature")));
            for a in acts {
                ui.label(
                    a.avg_temp
                        .map(|t| format!("{} °C", t.round()))
                        .unwrap_or_else(format::dash),
                );
            }
            ui.end_row();
            ui.label(weak(ui, tr!("Carga", "Load")));
            for a in acts {
                ui.label(
                    app.data
                        .metrics(a.id)
                        .map(|m| format!("{}", m.load.round()))
                        .unwrap_or_else(format::dash),
                );
            }
            ui.end_row();
        });
    if let Some(id) = open {
        app.go(Page::Activity(id));
    }
}

fn overlay(app: &mut App, ui: &mut Ui, acts: &[Activity]) {
    let st = &mut app.pages.compare;
    ui.horizontal(|ui| {
        ui.label(tr!("Sobrepor por:", "Overlay by:"));
        ui.selectable_value(&mut st.by_distance, true, tr!("Distância", "Distance"));
        ui.selectable_value(&mut st.by_distance, false, tr!("Tempo", "Time"));
    });
    let xd = st.by_distance;
    let xa = if xd { Axis::Distance } else { Axis::Time };
    let series_of = |pick: &dyn Fn(&Stream, usize) -> Option<f64>, smooth: usize| -> Vec<Series> {
        acts.iter()
            .enumerate()
            .filter_map(|(k, a)| {
                let s = st.streams.get(&a.id)?;
                let pts: Vec<[f64; 2]> = (0..s.len())
                    .filter_map(|i| {
                        let x = if xd { s.dist[i]? } else { s.t[i] };
                        Some([x, pick(s, i)?])
                    })
                    .collect();
                if pts.is_empty() {
                    return None;
                }
                Some(Series::line(
                    format::date(day_of(a)),
                    widgets::SERIES[k % 6],
                    charts::smooth(&pts, smooth),
                ))
            })
            .collect()
    };
    let mk = |id: &'static str, y: Axis, title: &'static str| {
        let mut o = ChartOpts::new(id, xa, y);
        o.link = Some("cmp");
        o.title = Some(title);
        o.legend = true;
        o.height = 200.0;
        o
    };
    charts::line_chart(
        ui,
        mk("cmp-hr", Axis::Bpm, tr!("Frequência cardíaca", "Heart rate")),
        series_of(&|s, i| s.hr[i], 2),
    );
    let pace = series_of(&|s, i| s.speed[i].filter(|v| *v > 0.8 && s.moving[i]), 7)
        .into_iter()
        .map(|mut s| {
            s.points.iter_mut().for_each(|p| p[1] = (1000.0 / p[1]).min(900.0));
            s
        })
        .collect();
    let mut o = mk("cmp-pace", Axis::Pace, tr!("Pace", "Pace"));
    o.invert_y = true;
    charts::line_chart(ui, o, pace);
    let cad = series_of(&|s, i| s.cadence[i], 5);
    if !cad.is_empty() {
        charts::line_chart(ui, mk("cmp-cad", Axis::Cadence, tr!("Cadência", "Cadence")), cad);
    }
    let alt = series_of(&|s, i| s.alt[i], 3);
    if !alt.is_empty() {
        charts::line_chart(ui, mk("cmp-alt", Axis::Meters, tr!("Altitude", "Altitude")), alt);
    }
}
