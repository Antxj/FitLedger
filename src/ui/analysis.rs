//! Analysis page: pace × HR (item 8), cardiac drift over time (item 9),
//! reference tests (item 10), pace ↔ speed converter (item 11) and the
//! heart-rate area (item 7).

use super::charts::{self, Axis, ChartOpts, Series};
use super::widgets::{self, weak};
use super::{App, Page};
use crate::analysis::load::day_of;
use crate::analysis::trends::{self, PACE_BIN};
use crate::format;
use crate::model::{Activity, Modality};
use crate::planning::RefTest;
use eframe::egui::{self, RichText, Ui};
use egui_extras::{Column, TableBuilder};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    PaceHr,
    Drift,
    Tests,
    Converter,
    HeartRate,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum RunFilter {
    #[default]
    All,
    Road,
    Treadmill,
}

impl RunFilter {
    fn ok(self, m: Modality) -> bool {
        match self {
            RunFilter::All => m.is_run(),
            RunFilter::Road => m == Modality::RoadRun,
            RunFilter::Treadmill => m == Modality::Treadmill,
        }
    }
}

#[derive(Default)]
pub struct State {
    pub tab: Tab,
    filter: RunFilter,
    pace_text: String,
    pace: Option<f64>,
    conv_pace: String,
    conv_kmh: String,
    test_edit: Option<TestEdit>,
    test_selected: Option<i64>,
    add_run: Option<i64>,
}

struct TestEdit {
    t: RefTest,
    distance: String,
    pace: String,
    incline: String,
}

fn filter_ui(ui: &mut Ui, f: &mut RunFilter) {
    ui.horizontal(|ui| {
        ui.label(tr!("Corridas:", "Runs:"));
        ui.selectable_value(f, RunFilter::All, tr!("Rua + esteira", "Road + treadmill"));
        ui.selectable_value(f, RunFilter::Road, Modality::RoadRun.label());
        ui.selectable_value(f, RunFilter::Treadmill, Modality::Treadmill.label());
    });
}

pub fn show(app: &mut App, ui: &mut Ui) {
    widgets::page_title(ui, tr!("Análises", "Analysis"));
    ui.horizontal(|ui| {
        let t = &mut app.pages.analysis.tab;
        ui.selectable_value(t, Tab::PaceHr, tr!("Pace × FC", "Pace × HR"));
        ui.selectable_value(t, Tab::Drift, tr!("Deriva cardíaca", "Cardiac drift"));
        ui.selectable_value(t, Tab::Tests, tr!("Testes de referência", "Reference tests"));
        ui.selectable_value(t, Tab::HeartRate, tr!("Frequência cardíaca", "Heart rate"));
        ui.selectable_value(t, Tab::Converter, tr!("Conversor pace ↔ km/h", "Pace ↔ km/h converter"));
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| match app.pages.analysis.tab {
            Tab::PaceHr => pace_hr(app, ui),
            Tab::Drift => drift(app, ui),
            Tab::Tests => tests(app, ui),
            Tab::Converter => converter(app, ui),
            Tab::HeartRate => heart_rate(app, ui),
        });
}

fn pace_hr(app: &mut App, ui: &mut Ui) {
    let s = &mut app.pages.analysis;
    ui.label(weak(ui, tr!(
        "Mostra se você está correndo mais rápido com a mesma FC, ou no mesmo ritmo com FC menor. Usa trechos estáveis de 1 minuto (pace com variação abaixo de 5%, depois dos 3 primeiros minutos), além da média de cada corrida.",
        "Shows whether you run faster at the same HR, or at the same pace with a lower HR. Uses steady 1-minute stretches (pace varying less than 5%, after the first 3 minutes), plus each run's average."
    )));
    filter_ui(ui, &mut s.filter);
    let f = s.filter;
    let runs: Vec<&Activity> = app.data.activities.iter().filter(|a| f.ok(a.modality)).collect();
    if runs.is_empty() {
        widgets::empty_state(ui, tr!("Nenhuma corrida ainda.", "No runs yet."));
        return;
    }
    let bands = trends::pace_bands(runs.iter().copied(), &app.data.metrics);
    if s.pace.is_none() {
        s.pace = trends::typical_pace(&bands).map(trends::pace_band).or(Some(360.0));
        s.pace_text = s.pace.map(format::pace).unwrap_or_default();
    }
    ui.horizontal(|ui| {
        ui.label(tr!("Pace analisado:", "Pace analysed:"));
        if ui
            .add(egui::TextEdit::singleline(&mut s.pace_text).desired_width(60.0))
            .changed()
            && let Some(p) = format::parse_pace(&s.pace_text)
        {
            s.pace = Some(p);
        }
        ui.label("/km");
        if let Some(p) = s.pace {
            ui.label(weak(
                ui,
                trf!(
                    "= {} km/h · faixa {}–{}",
                    "= {} km/h · range {}–{}",
                    format::num(format::pace_to_kmh(p), 2),
                    format::pace(p - PACE_BIN / 2.0),
                    format::pace(p + PACE_BIN / 2.0)
                ),
            ));
        }
    });
    let pace = s.pace.unwrap_or(360.0);
    let pts = trends::hr_at_pace(runs.iter().copied(), &app.data.metrics, pace);
    let notes = {
        let used: Vec<&Activity> = pts.iter().filter_map(|p| app.data.get(p.activity_id)).collect();
        trends::comparability_notes(&used, &app.data.metrics)
    };
    ui.columns(2, |c| {
        let ui = &mut c[0];
        widgets::section(
            ui,
            &trf!(
                "FC no pace {}/km ao longo do tempo",
                "HR at {}/km over time",
                format::pace(pace)
            ),
        );
        if pts.is_empty() {
            ui.label(weak(
                ui,
                tr!(
                    "Nenhuma corrida com trechos nesse pace.",
                    "No run with stretches at this pace."
                ),
            ));
        } else {
            let road: Vec<[f64; 2]> = pts
                .iter()
                .filter(|p| p.modality == Modality::RoadRun)
                .map(|p| [charts::date_to_x(p.date), p.hr])
                .collect();
            let tread: Vec<[f64; 2]> = pts
                .iter()
                .filter(|p| p.modality == Modality::Treadmill)
                .map(|p| [charts::date_to_x(p.date), p.hr])
                .collect();
            let all: Vec<[f64; 2]> = pts.iter().map(|p| [charts::date_to_x(p.date), p.hr]).collect();
            let mut o = ChartOpts::new("hr-at-pace", Axis::Date, Axis::Bpm);
            o.height = 220.0;
            o.legend = true;
            charts::line_chart(
                ui,
                o,
                vec![
                    Series::line(tr!("tendência", "trend"), egui::Color32::GRAY, all),
                    Series::dots(
                        Modality::RoadRun.label(),
                        widgets::modality_color(Modality::RoadRun),
                        road,
                    ),
                    Series::dots(
                        Modality::Treadmill.label(),
                        widgets::modality_color(Modality::Treadmill),
                        tread,
                    ),
                ],
            );
            egui::Grid::new("hr-at-pace-table").striped(true).show(ui, |ui| {
                for p in &pts {
                    ui.label(format::date(p.date));
                    ui.label(RichText::new(format::bpm(Some(p.hr))).strong());
                    ui.label(weak(ui, p.modality.label()));
                    ui.label(weak(
                        ui,
                        if p.minutes > 0 {
                            trf!("{} min estáveis", "{} steady min", p.minutes)
                        } else {
                            tr!("média da corrida", "run average").into()
                        },
                    ));
                    ui.end_row();
                }
            });
        }
        let ui = &mut c[1];
        widgets::section(ui, tr!("FC média por faixa de pace", "Average HR by pace band"));
        if bands.is_empty() {
            ui.label(weak(
                ui,
                tr!("Ainda não há trechos estáveis.", "No steady stretches yet."),
            ));
        } else {
            let shown: Vec<_> = bands.iter().filter(|b| b.minutes >= 2).collect();
            let pts: Vec<[f64; 2]> = shown.iter().map(|b| [b.pace, b.avg_hr]).collect();
            let mut o = ChartOpts::new("bands", Axis::Pace, Axis::Bpm);
            o.height = 220.0;
            o.invert_x = true;
            charts::line_chart(
                ui,
                o,
                vec![
                    Series::line(tr!("FC média", "Avg HR"), widgets::HR, pts.clone()),
                    Series::dots(tr!("FC média", "Avg HR"), widgets::HR, pts),
                ],
            );
            ui.label(weak(ui, tr!("Mais à direita = mais rápido.", "Further right = faster.")).small());
            egui::Grid::new("bands-table").striped(true).show(ui, |ui| {
                for b in shown.iter().rev() {
                    ui.label(format!("{}/km", format::pace(b.pace)));
                    ui.label(RichText::new(format::bpm(Some(b.avg_hr))).strong());
                    ui.label(weak(
                        ui,
                        trf!("{} min · {} corridas", "{} min · {} runs", b.minutes, b.activities),
                    ));
                    ui.end_row();
                }
            });
        }
    });
    for n in notes {
        widgets::note(ui, &format!("⚠ {n}"), widgets::WARN);
    }
    widgets::section(
        ui,
        tr!(
            "Todos os trechos estáveis (FC × pace)",
            "All steady stretches (HR × pace)"
        ),
    );
    let mut series = Vec::new();
    let recent_cut = chrono::Local::now().date_naive() - chrono::Duration::days(60);
    let mut old = Vec::new();
    let mut new = Vec::new();
    for a in &runs {
        if let Some(m) = app.data.metrics(a.id) {
            for p in &m.pace_hr {
                if day_of(a) >= recent_cut {
                    new.push(*p)
                } else {
                    old.push(*p)
                }
            }
        }
    }
    series.push(Series::dots(
        tr!("antes", "earlier"),
        egui::Color32::from_gray(140),
        old,
    ));
    series.push(Series::dots(
        tr!("últimos 60 dias", "last 60 days"),
        widgets::ACCENT,
        new,
    ));
    let mut o = ChartOpts::new("scatter", Axis::Pace, Axis::Bpm);
    o.height = 260.0;
    o.invert_x = true;
    o.legend = true;
    charts::line_chart(ui, o, series);
    ui.label(
        weak(
            ui,
            tr!(
                "Eixo do pace invertido: pontos mais à direita e mais baixos = mais rápido com FC menor.",
                "Inverted pace axis: points further right and lower = faster with a lower HR."
            ),
        )
        .small(),
    );
}

fn drift(app: &mut App, ui: &mut Ui) {
    let s = &mut app.pages.analysis;
    ui.label(weak(ui, tr!(
        "Desacoplamento Pa:FC de cada corrida (≥ 10 min): quanto a relação velocidade/FC piorou da 1ª para a 2ª metade. Abaixo de ~5% indica boa eficiência aeróbica naquele ritmo.",
        "Pa:HR decoupling of each run (≥ 10 min): how much the speed/HR ratio worsened from the 1st to the 2nd half. Below ~5% indicates good aerobic efficiency at that pace."
    )));
    filter_ui(ui, &mut s.filter);
    let f = s.filter;
    let mut rows: Vec<(&Activity, &crate::analysis::hr::Drift)> = app
        .data
        .activities
        .iter()
        .filter(|a| f.ok(a.modality))
        .filter_map(|a| Some((a, app.data.metrics(a.id)?.drift.as_ref()?)))
        .collect();
    rows.sort_by_key(|(a, _)| a.start_time);
    if rows.is_empty() {
        widgets::empty_state(
            ui,
            tr!("Nenhuma corrida com dados suficientes.", "No run with enough data."),
        );
        return;
    }
    let pts: Vec<[f64; 2]> = rows
        .iter()
        .filter_map(|(a, d)| Some([charts::date_to_x(day_of(a)), d.decoupling_pct?]))
        .collect();
    let mut o = ChartOpts::new("drift-trend", Axis::Date, Axis::Percent);
    o.height = 200.0;
    o.hlines.push((5.0, "5%".into()));
    charts::line_chart(
        ui,
        o,
        vec![
            Series::line(tr!("desacoplamento", "decoupling"), widgets::WARN, pts.clone()),
            Series::dots("", widgets::WARN, pts),
        ],
    );
    let mut open = None;
    let pace = |v: Option<f64>| {
        v.filter(|x| *x > 0.3)
            .map(|x| format::pace_unit(1000.0 / x))
            .unwrap_or_else(format::dash)
    };
    TableBuilder::new(ui)
        .striped(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .columns(Column::initial(105.0).at_least(70.0), 8)
        .header(24.0, |mut h| {
            for t in [
                tr!("Data", "Date"),
                tr!("Tipo", "Type"),
                tr!("Distância", "Distance"),
                tr!("FC 1ª metade", "HR 1st half"),
                tr!("FC 2ª metade", "HR 2nd half"),
                tr!("Pace 1ª metade", "Pace 1st half"),
                tr!("Pace 2ª metade", "Pace 2nd half"),
                tr!("Desacoplamento", "Decoupling"),
            ] {
                h.col(|ui| {
                    ui.label(RichText::new(t).strong());
                });
            }
        })
        .body(|mut body| {
            for (a, d) in rows.iter().rev() {
                body.row(24.0, |mut row| {
                    row.col(|ui| {
                        ui.label(format::date(day_of(a)));
                    });
                    row.col(|ui| {
                        ui.label(a.modality.label());
                    });
                    row.col(|ui| {
                        ui.label(format::opt_km(a.distance()));
                    });
                    row.col(|ui| {
                        ui.label(format::bpm(Some(d.hr1)));
                    });
                    row.col(|ui| {
                        ui.label(format::bpm(Some(d.hr2)));
                    });
                    row.col(|ui| {
                        ui.label(pace(d.speed1));
                    });
                    row.col(|ui| {
                        ui.label(pace(d.speed2));
                    });
                    row.col(|ui| {
                        let v = d.decoupling_pct.unwrap_or(0.0);
                        let c = if v < 5.0 {
                            widgets::GOOD
                        } else if v < 10.0 {
                            widgets::WARN
                        } else {
                            widgets::BAD
                        };
                        ui.label(RichText::new(format!("{}%", format::num(v, 1))).color(c));
                    });
                    if row.response().clicked() {
                        open = Some(a.id);
                    }
                });
            }
        });
    if let Some(id) = open {
        app.go(Page::Activity(id));
    }
}

fn tests(app: &mut App, ui: &mut Ui) {
    let tests = app.db.reference_tests().unwrap_or_default();
    ui.label(weak(ui, tr!(
        "Um teste de referência é sempre feito nas mesmas condições (ex.: 5 km a 6:00/km na esteira a 10 km/h, 1% de inclinação). Comparando as execuções, a queda da FC mostra a evolução do condicionamento.",
        "A reference test is always done under the same conditions (e.g. 5 km at 6:00/km on the treadmill at 10 km/h, 1% incline). Comparing runs, a lower HR shows fitness progress."
    )));
    ui.horizontal(|ui| {
        for t in &tests {
            if ui
                .selectable_label(app.pages.analysis.test_selected == Some(t.id), &t.name)
                .clicked()
            {
                app.pages.analysis.test_selected = Some(t.id);
                app.pages.analysis.test_edit = None;
            }
        }
        if ui.button(tr!("➕ Novo teste", "➕ New test")).clicked() {
            app.pages.analysis.test_edit = Some(TestEdit {
                t: RefTest {
                    name: tr!("5 km a 6:00/km", "5 km at 6:00/km").into(),
                    modality: Modality::Treadmill,
                    distance_m: 5000.0,
                    target_pace: Some(360.0),
                    incline_pct: Some(1.0),
                    ..Default::default()
                },
                distance: "5".into(),
                pace: "6:00".into(),
                incline: "1".into(),
            });
        }
    });
    if app.pages.analysis.test_selected.is_none() {
        app.pages.analysis.test_selected = tests.first().map(|t| t.id);
    }
    if let Some(e) = app.pages.analysis.test_edit.as_mut() {
        let mut save = false;
        let mut cancel = false;
        widgets::card_frame(ui).show(ui, |ui| {
            egui::Grid::new("test-edit")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr!("Nome", "Name"));
                    ui.text_edit_singleline(&mut e.t.name);
                    ui.end_row();
                    ui.label(tr!("Onde", "Where"));
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut e.t.modality, Modality::Treadmill, Modality::Treadmill.label());
                        ui.selectable_value(&mut e.t.modality, Modality::RoadRun, Modality::RoadRun.label());
                    });
                    ui.end_row();
                    ui.label(tr!("Distância (km)", "Distance (km)"));
                    ui.add(egui::TextEdit::singleline(&mut e.distance).desired_width(60.0));
                    ui.end_row();
                    ui.label(tr!("Pace alvo", "Target pace"));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut e.pace).desired_width(60.0));
                        if let Some(p) = format::parse_pace(&e.pace) {
                            ui.label(weak(
                                ui,
                                trf!(
                                    "= {} km/h na esteira",
                                    "= {} km/h on the treadmill",
                                    format::num(format::pace_to_kmh(p), 1)
                                ),
                            ));
                        }
                    });
                    ui.end_row();
                    if e.t.modality == Modality::Treadmill {
                        ui.label(tr!("Inclinação (%)", "Incline (%)"));
                        ui.add(egui::TextEdit::singleline(&mut e.incline).desired_width(60.0));
                        ui.end_row();
                    }
                });
            ui.horizontal(|ui| {
                save = ui.button(tr!("💾 Salvar", "💾 Save")).clicked();
                cancel = ui.button(tr!("Cancelar", "Cancel")).clicked();
            });
        });
        if save {
            e.t.distance_m = format::parse_num(&e.distance).map(|v| v * 1000.0).unwrap_or(5000.0);
            e.t.target_pace = format::parse_pace(&e.pace);
            e.t.incline_pct = format::parse_num(&e.incline);
            let t = e.t.clone();
            match app.db.save_reference_test(&t) {
                Ok(id) => {
                    app.pages.analysis.test_selected = Some(id);
                    app.pages.analysis.test_edit = None;
                }
                Err(err) => app.toast_error(err.to_string()),
            }
        } else if cancel {
            app.pages.analysis.test_edit = None;
        }
        return;
    }
    let Some(test) = tests
        .iter()
        .find(|t| Some(t.id) == app.pages.analysis.test_selected)
        .cloned()
    else {
        widgets::empty_state(ui, tr!("Crie um teste para começar.", "Create a test to start."));
        return;
    };
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(&test.name).size(17.0).strong());
        let mut desc = format!("{} · {}", test.modality.label(), format::km(test.distance_m));
        if let Some(p) = test.target_pace {
            desc.push_str(&format!(
                " · {} ({} km/h)",
                format::pace_unit(p),
                format::num(format::pace_to_kmh(p), 1)
            ));
        }
        if let Some(i) = test.incline_pct {
            desc.push_str(&trf!(" · inclinação {}%", " · incline {}%", format::num_trim(i, 1)));
        }
        ui.label(weak(ui, desc));
        if ui.small_button(tr!("✏ Editar", "✏ Edit")).clicked() {
            app.pages.analysis.test_edit = Some(TestEdit {
                distance: format::num_trim(test.distance_m / 1000.0, 2),
                pace: test.target_pace.map(format::pace).unwrap_or_default(),
                incline: test.incline_pct.map(|v| format::num_trim(v, 1)).unwrap_or_default(),
                t: test.clone(),
            });
        }
        if ui
            .small_button(RichText::new(tr!("🗑 Excluir", "🗑 Delete")).color(widgets::BAD))
            .clicked()
        {
            let _ = app.db.delete_reference_test(test.id);
            app.pages.analysis.test_selected = None;
        }
    });
    let run_ids = app.db.reference_runs(test.id).unwrap_or_default();
    // add an execution
    ui.horizontal(|ui| {
        ui.label(tr!("Adicionar execução:", "Add a run:"));
        let candidates: Vec<&Activity> = app
            .data
            .activities
            .iter()
            .filter(|a| a.modality.is_run() && !run_ids.contains(&a.id))
            .filter(|a| {
                a.distance()
                    .is_some_and(|d| (d - test.distance_m).abs() <= test.distance_m * 0.15)
            })
            .collect();
        let sel = &mut app.pages.analysis.add_run;
        let label = sel
            .and_then(|id| app.data.get(id))
            .map(|a| format!("{} · {}", format::date(day_of(a)), a.modality.label()));
        egui::ComboBox::from_id_salt("add-run")
            .selected_text(label.unwrap_or_else(|| tr!("escolha uma corrida", "choose a run").into()))
            .width(260.0)
            .show_ui(ui, |ui| {
                for a in candidates {
                    let text = format!(
                        "{} · {} · {} · {}",
                        format::date(day_of(a)),
                        a.modality.label(),
                        format::opt_km(a.distance()),
                        format::opt_pace(a.pace_s_per_km())
                    );
                    ui.selectable_value(sel, Some(a.id), text);
                }
            });
        if ui
            .add_enabled(sel.is_some(), egui::Button::new(tr!("Adicionar", "Add")))
            .clicked()
            && let Some(id) = sel.take()
        {
            let _ = app.db.set_reference_run(test.id, id, true);
        }
    });
    let runs: Vec<&Activity> = run_ids.iter().filter_map(|id| app.data.get(*id)).collect();
    if runs.is_empty() {
        ui.label(weak(ui, tr!("Nenhuma execução ainda.", "No runs yet.")));
        return;
    }
    let kms = (test.distance_m / 1000.0).round().max(1.0) as usize;
    let pts: Vec<[f64; 2]> = runs
        .iter()
        .filter_map(|a| Some([charts::date_to_x(day_of(a)), a.avg_hr?]))
        .collect();
    let mut o = ChartOpts::new("test-hist", Axis::Date, Axis::Bpm);
    o.height = 200.0;
    o.title = Some(tr!("FC média em cada execução", "Average HR in each run"));
    charts::line_chart(
        ui,
        o,
        vec![
            Series::line(tr!("FC média", "Avg HR"), widgets::HR, pts.clone()),
            Series::dots(tr!("FC média", "Avg HR"), widgets::HR, pts),
        ],
    );
    let mut remove = None;
    let mut open = None;
    let notes = trends::comparability_notes(&runs, &app.data.metrics);
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(60.0))
        .columns(Column::initial(84.0).at_least(60.0), 4)
        .columns(Column::initial(66.0).at_least(50.0), kms)
        .column(Column::initial(80.0))
        .column(Column::remainder())
        .header(24.0, |mut h| {
            h.col(|ui| {
                ui.label(RichText::new("#").strong());
            });
            for t in [
                tr!("Data", "Date"),
                "Pace",
                tr!("FC média", "Avg HR"),
                tr!("FC máxima", "Max HR"),
            ] {
                h.col(|ui| {
                    ui.label(RichText::new(t).strong());
                });
            }
            for k in 1..=kms {
                h.col(|ui| {
                    ui.label(RichText::new(trf!("FC km {k}", "HR km {k}")).strong());
                });
            }
            h.col(|ui| {
                ui.label(RichText::new(tr!("Deriva", "Drift")).strong());
            });
            h.col(|_| {});
        })
        .body(|mut body| {
            for (i, a) in runs.iter().enumerate() {
                let m = app.data.metrics(a.id);
                body.row(24.0, |mut row| {
                    row.col(|ui| {
                        if ui.link(trf!("Teste {}", "Test {}", i + 1)).clicked() {
                            open = Some(a.id);
                        }
                    });
                    row.col(|ui| {
                        ui.label(format::date(day_of(a)));
                    });
                    row.col(|ui| {
                        ui.label(format::opt_pace(a.pace_s_per_km()));
                    });
                    row.col(|ui| {
                        ui.label(RichText::new(format::bpm(a.avg_hr)).strong());
                    });
                    row.col(|ui| {
                        ui.label(format::bpm(a.max_hr));
                    });
                    for k in 0..kms {
                        row.col(|ui| {
                            ui.label(format::bpm(m.and_then(|m| m.km_hr.get(k).copied().flatten())));
                        });
                    }
                    row.col(|ui| {
                        ui.label(
                            m.and_then(|m| m.drift.as_ref())
                                .and_then(|d| d.decoupling_pct)
                                .map(|v| format!("{}%", format::num(v, 1)))
                                .unwrap_or_else(format::dash),
                        );
                    });
                    row.col(|ui| {
                        if ui
                            .small_button("✖")
                            .on_hover_text(tr!("Remover do teste", "Remove from the test"))
                            .clicked()
                        {
                            remove = Some(a.id);
                        }
                    });
                });
            }
        });
    if runs.len() >= 2 {
        let first = runs[0].avg_hr.unwrap_or(0.0);
        let last = runs[runs.len() - 1].avg_hr.unwrap_or(0.0);
        if first > 0.0 && last > 0.0 {
            let d = last - first;
            widgets::note(
                ui,
                &trf!(
                    "Da primeira à última execução: {} bpm de FC média.",
                    "From the first to the latest run: {} bpm of average HR.",
                    if d > 0.0 {
                        format!("+{}", d.round())
                    } else {
                        format!("{}", d.round())
                    }
                ),
                if d <= 0.0 { widgets::GOOD } else { widgets::WARN },
            );
        }
    }
    for n in notes {
        widgets::note(ui, &format!("⚠ {n}"), widgets::WARN);
    }
    if let Some(id) = remove {
        let _ = app.db.set_reference_run(test.id, id, false);
    }
    if let Some(id) = open {
        app.go(Page::Activity(id));
    }
}

fn converter(app: &mut App, ui: &mut Ui) {
    let s = &mut app.pages.analysis;
    ui.horizontal(|ui| {
        ui.label(tr!("Pace", "Pace"));
        if ui
            .add(
                egui::TextEdit::singleline(&mut s.conv_pace)
                    .hint_text("6:00")
                    .desired_width(70.0),
            )
            .changed()
            && let Some(p) = format::parse_pace(&s.conv_pace)
        {
            s.conv_kmh = format::num(format::pace_to_kmh(p), 2);
        }
        ui.label("/km  =");
        if ui
            .add(
                egui::TextEdit::singleline(&mut s.conv_kmh)
                    .hint_text("10,0")
                    .desired_width(70.0),
            )
            .changed()
            && let Some(v) = format::parse_num(&s.conv_kmh).filter(|v| *v > 0.5)
        {
            s.conv_pace = format::pace(format::kmh_to_pace(v));
        }
        ui.label("km/h");
    });
    ui.add_space(8.0);
    egui::Grid::new("conv-table")
        .striped(true)
        .min_col_width(110.0)
        .show(ui, |ui| {
            ui.label(RichText::new("Pace").strong());
            ui.label(RichText::new(tr!("Velocidade (esteira)", "Speed (treadmill)")).strong());
            ui.label(RichText::new(tr!("Tempo em 5 km", "5 km time")).strong());
            ui.label(RichText::new(tr!("Tempo em 10 km", "10 km time")).strong());
            ui.end_row();
            let mut p = 480.0;
            while p >= 270.0 {
                ui.label(format::pace_unit(p));
                ui.label(format!("{} km/h", format::num(format::pace_to_kmh(p), 2)));
                ui.label(format::duration(p * 5.0));
                ui.label(format::duration(p * 10.0));
                ui.end_row();
                p -= 15.0;
            }
        });
}

fn heart_rate(app: &mut App, ui: &mut Ui) {
    let observed = app.db.observed_max_hr();
    let zones = app.settings.zones(observed);
    ui.horizontal(|ui| {
        ui.label(trf!(
            "FC máxima usada: {} · FC de repouso: {}",
            "Max HR used: {} · Resting HR: {}",
            format::bpm(Some(zones.max)),
            format::bpm(Some(app.settings.effective_hr_rest()))
        ));
        if ui.link(tr!("alterar em Configurações", "change in Settings")).clicked() {
            app.go(Page::Settings);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for z in 1..=5 {
            let (lo, hi) = zones.range(z);
            widgets::chip(
                ui,
                &format!("Z{z}: {}–{} bpm", lo.round(), hi.round()),
                widgets::ZONES[z],
            );
        }
    });
    widgets::section(
        ui,
        tr!("Tempo em cada zona por treino", "Time in each zone per workout"),
    );
    let mut open = None;
    for a in app.data.activities.iter().filter(|a| a.avg_hr.is_some()).take(40) {
        let Some(m) = app.data.metrics(a.id) else { continue };
        let total: f64 = m.zones_s.iter().sum();
        if total <= 0.0 {
            continue;
        }
        ui.horizontal(|ui| {
            if ui
                .add_sized(
                    [190.0, 18.0],
                    egui::Link::new(format!("{} · {}", format::date(day_of(a)), a.modality.label())),
                )
                .clicked()
            {
                open = Some(a.id);
            }
            let w = (ui.available_width() - 70.0).max(100.0);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 16.0), egui::Sense::hover());
            let mut x = rect.left();
            let mut tip = Vec::new();
            for z in 0..6 {
                let frac = (m.zones_s[z] / total) as f32;
                if frac <= 0.0 {
                    continue;
                }
                let wz = frac * rect.width();
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(wz, rect.height())),
                    2.0,
                    widgets::ZONES[z],
                );
                x += wz;
                tip.push(format!(
                    "{}: {} ({}%)",
                    if z == 0 { "<Z1".to_string() } else { format!("Z{z}") },
                    format::duration(m.zones_s[z]),
                    format::num(frac as f64 * 100.0, 0)
                ));
            }
            resp.on_hover_text(tip.join("\n"));
            ui.label(format::bpm(a.avg_hr));
        });
    }
    if let Some(id) = open {
        app.pages.activity.open_tab = Some(super::activity::Tab::Zones);
        app.go(Page::Activity(id));
    }
}
