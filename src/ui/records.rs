//! Records (item 25): best efforts detected automatically in every run,
//! road and treadmill shown separately (treadmill distance is estimated).

use super::widgets::{self, weak};
use super::{App, Page};
use crate::analysis::load::day_of;
use crate::analysis::splits::{MILE_M, RECORD_DISTANCES};
use crate::format;
use crate::model::Modality;
use eframe::egui::{self, RichText, Ui};

fn dist_label(d: f64) -> String {
    if (d - MILE_M).abs() < 1.0 {
        tr!("1 milha", "1 mile").into()
    } else if d < 1000.0 {
        format!("{} m", d.round())
    } else {
        format!("{} km", format::num_trim(d / 1000.0, 1))
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    widgets::page_title(ui, tr!("Records", "Records"));
    ui.label(weak(ui, tr!(
        "Melhores marcas encontradas dentro de qualquer corrida (o trecho mais rápido com aquela distância). A esteira fica separada porque a distância é estimada pelo relógio.",
        "Best efforts found inside any run (the fastest stretch with that distance). The treadmill is kept apart because its distance is estimated by the watch."
    )));
    let mut efforts: Vec<(i64, f64, f64)> = Vec::new();
    if let Ok(mut st) = app
        .db
        .conn
        .prepare("SELECT activity_id, distance_m, time_s FROM best_effort")
        && let Ok(rows) = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    {
        efforts = rows.flatten().collect();
    }
    let mut open = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for m in [Modality::RoadRun, Modality::Treadmill] {
            widgets::section(ui, m.label());
            egui::Grid::new(format!("rec-{}", m.code()))
                .striped(true)
                .min_col_width(110.0)
                .show(ui, |ui| {
                    for t in [
                        tr!("Distância", "Distance"),
                        tr!("Tempo", "Time"),
                        "Pace",
                        tr!("Data", "Date"),
                        tr!("Atividade de origem", "Source activity"),
                        tr!("2º e 3º melhores", "2nd and 3rd best"),
                    ] {
                        ui.label(RichText::new(t).strong());
                    }
                    ui.end_row();
                    for d in RECORD_DISTANCES {
                        let mut list: Vec<(i64, f64)> = efforts
                            .iter()
                            .filter(|(_, dd, _)| (dd - d).abs() < 1.0)
                            .filter(|(id, _, _)| app.data.get(*id).is_some_and(|a| a.modality == m))
                            .map(|(id, _, t)| (*id, *t))
                            .collect();
                        list.sort_by(|a, b| a.1.total_cmp(&b.1));
                        ui.label(RichText::new(dist_label(d)).strong());
                        match list.first() {
                            Some((id, t)) => {
                                let a = app.data.get(*id).unwrap();
                                ui.label(RichText::new(format::duration(*t)).strong().color(widgets::GOOD));
                                ui.label(format::pace_unit(t / (d / 1000.0)));
                                ui.label(format::date(day_of(a)));
                                if ui
                                    .link(format!("{} · {}", a.title(), format::opt_km(a.distance())))
                                    .clicked()
                                {
                                    open = Some(*id);
                                }
                                let rest: Vec<String> = list
                                    .iter()
                                    .skip(1)
                                    .take(2)
                                    .filter_map(|(id, t)| {
                                        app.data.get(*id).map(|a| {
                                            format!("{} ({})", format::duration(*t), format::date_short(day_of(a)))
                                        })
                                    })
                                    .collect();
                                ui.label(weak(ui, rest.join(" · ")));
                            }
                            None => {
                                for _ in 0..5 {
                                    ui.label(weak(ui, format::dash()));
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
        }
    });
    if let Some(id) = open {
        app.go(Page::Activity(id));
    }
}
