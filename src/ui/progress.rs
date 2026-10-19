//! Analysis: progress toward the goal, weekly volume, pace × HR, cardiac
//! drift, records, heart-rate zones and the pace ↔ speed converter.

use super::charts::{self, Axis, ChartOpts, Series};
use super::widgets::{self, weak};
use super::{App, Page};
use crate::analysis::splits::{MILE_M, RECORD_DISTANCES};
use crate::analysis::trends::{self, PACE_BIN};
use crate::coach::fitness::day as day_of;
use crate::format;
use crate::model::{Activity, Modality};
use eframe::egui::{self, RichText, Ui};
use egui_extras::{Column, TableBuilder};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Goal,
    Volume,
    PaceHr,
    Drift,
    Records,
    HeartRate,
    Converter,
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
    ui.horizontal_wrapped(|ui| {
        let t = &mut app.pages.progress.tab;
        ui.selectable_value(t, Tab::Goal, tr!("Evolução da meta", "Goal progress"));
        ui.selectable_value(t, Tab::Volume, tr!("Volume", "Volume"));
        ui.selectable_value(t, Tab::PaceHr, tr!("Pace × FC", "Pace × HR"));
        ui.selectable_value(t, Tab::Drift, tr!("Deriva cardíaca", "Cardiac drift"));
        ui.selectable_value(t, Tab::Records, tr!("Records", "Records"));
        ui.selectable_value(t, Tab::HeartRate, tr!("Zonas de FC", "HR zones"));
        ui.selectable_value(t, Tab::Converter, tr!("Conversor pace ↔ km/h", "Pace ↔ km/h converter"));
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| match app.pages.progress.tab {
            Tab::Goal => goal(app, ui),
            Tab::Volume => volume(app, ui),
            Tab::PaceHr => pace_hr(app, ui),
            Tab::Drift => drift(app, ui),
            Tab::Records => records(app, ui),
            Tab::Converter => converter(app, ui),
            Tab::HeartRate => heart_rate(app, ui),
        });
}

fn pace_hr(app: &mut App, ui: &mut Ui) {
    let s = &mut app.pages.progress;
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
    let s = &mut app.pages.progress;
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

fn converter(app: &mut App, ui: &mut Ui) {
    let s = &mut app.pages.progress;
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

fn goal(app: &mut App, ui: &mut Ui) {
    let Some(c) = app.coach.clone() else {
        widgets::empty_state(
            ui,
            tr!(
                "Defina uma meta para ver a evolução.",
                "Set a goal to see the progress."
            ),
        );
        return;
    };
    ui.label(RichText::new(c.goal.describe()).size(18.0).strong());
    widgets::card_grid(
        ui,
        200.0,
        &c.projection
            .parts
            .iter()
            .map(|p| {
                (
                    p.label.clone(),
                    p.now.clone(),
                    Some(trf!(
                        "meta {} · {}%",
                        "goal {} · {}%",
                        p.goal,
                        (p.fraction * 100.0).round()
                    )),
                )
            })
            .collect::<Vec<_>>(),
    );
    let pace = c.goal.goal_pace().unwrap_or(c.fitness.easy_pace);
    widgets::section(
        ui,
        &trf!(
            "FC estimada no pace {}",
            "Estimated HR at {} pace",
            format::pace_unit(pace)
        ),
    );
    ui.label(weak(
        ui,
        tr!(
            "Cada ponto é uma corrida, ajustada para o mesmo pace pela relação FC × velocidade das suas corridas. Descendo = mais eficiente.",
            "Each point is a run, adjusted to the same pace using the HR × speed relation of your runs. Going down = more efficient."
        ),
    ));
    let pts: Vec<[f64; 2]> = c
        .fitness
        .hr_history
        .iter()
        .map(|(d, h)| [charts::date_to_x(*d), *h])
        .collect();
    let mut o = ChartOpts::new("goal-hr", Axis::Date, Axis::Bpm);
    o.height = 240.0;
    if let Some(t) = c.goal.target_hr {
        o.hlines.push((t, tr!("meta", "goal").into()));
    }
    let mut series = vec![
        Series::line(tr!("FC", "HR"), widgets::HR, pts.clone()),
        Series::dots(tr!("FC", "HR"), widgets::HR, pts),
    ];
    // trend line and its projection
    // projection: only while the HR still has to come down to the goal
    if let (Some(slope), Some(last), Some(target)) = (
        c.fitness.hr_trend_per_week,
        c.fitness.hr_history.last(),
        c.goal.target_hr,
    ) && slope < 0.0
        && last.1 > target
    {
        let x0 = charts::date_to_x(last.0);
        let weeks = ((last.1 - target) / -slope).min(26.0);
        let end = x0 + weeks * 7.0;
        let y_end = target;
        series.push(Series::line(
            tr!("projeção", "projection"),
            egui::Color32::GRAY,
            vec![[x0, last.1], [end, y_end]],
        ));
    }
    charts::line_chart(ui, o, series);
    widgets::section(ui, tr!("Longão e tempo previsto", "Long run and predicted time"));
    let runs: Vec<&Activity> = app
        .data
        .activities
        .iter()
        .filter(|a| day_of(a) >= c.goal.start_date - chrono::Duration::days(28))
        .collect();
    let mut longest: std::collections::BTreeMap<chrono::NaiveDate, f64> = Default::default();
    for a in &runs {
        let e = longest.entry(crate::coach::week_start(day_of(a))).or_default();
        *e = e.max(a.distance().unwrap_or(0.0));
    }
    ui.columns(2, |cols| {
        let bars = longest
            .iter()
            .map(|(d, m)| {
                (
                    charts::date_to_x(*d),
                    *m,
                    widgets::PACE,
                    format!("{} · {}", format::date_short(*d), format::km(*m)),
                )
            })
            .collect();
        let mut o = ChartOpts::new("goal-long", Axis::Date, Axis::Distance);
        o.title = Some(tr!("Maior corrida por semana", "Longest run per week"));
        o.height = 180.0;
        o.hlines.push((c.goal.distance_m, tr!("meta", "goal").into()));
        charts::bar_chart(&mut cols[0], o, bars, 5.0);
        // predicted time by run (Riegel from each run)
        let pred: Vec<[f64; 2]> = runs
            .iter()
            .filter_map(|a| {
                let d = a.distance()?;
                let t = a.duration()?;
                (d >= 1000.0).then(|| [charts::date_to_x(day_of(a)), t * (c.goal.distance_m / d).powf(1.06)])
            })
            .collect();
        let mut o = ChartOpts::new("goal-pred", Axis::Date, Axis::Time);
        o.title = Some(tr!(
            "Tempo previsto para a distância (por corrida)",
            "Predicted time for the distance (per run)"
        ));
        o.height = 180.0;
        o.invert_y = true;
        if let Some(t) = c.goal.target_time_s {
            o.hlines.push((t, tr!("meta", "goal").into()));
        }
        charts::line_chart(
            &mut cols[1],
            o,
            vec![Series::dots(tr!("previsão", "prediction"), widgets::SPEED, pred)],
        );
    });
}

fn volume(app: &mut App, ui: &mut Ui) {
    let today = chrono::Local::now().date_naive();
    let from = today - chrono::Duration::days(7 * 16);
    let runs: Vec<&Activity> = app.data.activities.iter().filter(|a| day_of(a) >= from).collect();
    let w = crate::report::weeks(&runs, app.coach.as_ref(), from, today);
    ui.label(weak(ui, tr!("Últimas 16 semanas.", "Last 16 weeks.")));
    ui.columns(2, |cols| {
        let km = w
            .iter()
            .map(|(d, x)| {
                (
                    charts::date_to_x(*d),
                    x.km,
                    widgets::PACE,
                    format!(
                        "{} · {} km · {} corridas",
                        format::date_short(*d),
                        format::num(x.km, 1),
                        x.runs
                    ),
                )
            })
            .collect();
        let mut o = ChartOpts::new("vol-km", Axis::Date, Axis::Plain);
        o.title = Some(tr!("Km por semana", "Km per week"));
        o.height = 200.0;
        charts::bar_chart(&mut cols[0], o, km, 5.0);
        let h = w
            .iter()
            .map(|(d, x)| {
                (
                    charts::date_to_x(*d),
                    x.secs / 3600.0,
                    widgets::ACCENT,
                    format!("{} · {}", format::date_short(*d), format::hours_short(x.secs)),
                )
            })
            .collect();
        let mut o = ChartOpts::new("vol-h", Axis::Date, Axis::Hours);
        o.title = Some(tr!("Horas por semana", "Hours per week"));
        o.height = 200.0;
        charts::bar_chart(&mut cols[1], o, h, 5.0);
    });
    let adherence: Vec<[f64; 2]> = w
        .iter()
        .filter(|(_, x)| x.planned > 0)
        .map(|(d, x)| [charts::date_to_x(*d), x.done as f64 / x.planned as f64 * 100.0])
        .collect();
    if !adherence.is_empty() {
        let mut o = ChartOpts::new("vol-adh", Axis::Date, Axis::Percent);
        o.title = Some(tr!("Treinos do plano feitos (%)", "Plan sessions done (%)"));
        o.height = 160.0;
        charts::line_chart(
            ui,
            o,
            vec![
                Series::line("%", widgets::GOOD, adherence.clone()),
                Series::dots("%", widgets::GOOD, adherence),
            ],
        );
    }
    let hr: Vec<[f64; 2]> = w
        .iter()
        .filter(|(_, x)| x.hr_w > 0.0)
        .map(|(d, x)| [charts::date_to_x(*d), x.hr_sum / x.hr_w])
        .collect();
    let mut o = ChartOpts::new("vol-hr", Axis::Date, Axis::Bpm);
    o.title = Some(tr!("FC média por semana", "Average HR per week"));
    o.height = 160.0;
    charts::line_chart(
        ui,
        o,
        vec![
            Series::line(tr!("FC", "HR"), widgets::HR, hr.clone()),
            Series::dots(tr!("FC", "HR"), widgets::HR, hr),
        ],
    );
}

fn dist_label(d: f64) -> String {
    if (d - MILE_M).abs() < 1.0 {
        tr!("1 milha", "1 mile").into()
    } else if d < 1000.0 {
        format!("{} m", d.round())
    } else {
        format!("{} km", format::num_trim(d / 1000.0, 1))
    }
}

fn records(app: &mut App, ui: &mut Ui) {
    ui.label(weak(
        ui,
        tr!(
            "O trecho mais rápido de cada distância, dentro de qualquer corrida. A esteira fica separada (a distância é estimada pelo relógio).",
            "The fastest stretch of each distance, inside any run. The treadmill is kept apart (its distance is estimated by the watch)."
        ),
    ));
    let efforts = app.db.best_efforts();
    let mut open = None;
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
                    tr!("Corrida", "Run"),
                ] {
                    ui.label(RichText::new(t).strong());
                }
                ui.end_row();
                for d in RECORD_DISTANCES {
                    let best = efforts
                        .iter()
                        .filter(|(id, _)| app.data.get(**id).is_some_and(|a| a.modality == m))
                        .filter_map(|(id, l)| l.iter().find(|(dd, _)| (dd - d).abs() < 1.0).map(|(_, t)| (*id, *t)))
                        .min_by(|a, b| a.1.total_cmp(&b.1));
                    ui.label(RichText::new(dist_label(d)).strong());
                    match best.and_then(|(id, t)| Some((app.data.get(id)?, t))) {
                        Some((a, t)) => {
                            ui.label(RichText::new(format::duration(t)).strong().color(widgets::GOOD));
                            ui.label(format::pace_unit(t / (d / 1000.0)));
                            ui.label(format::date(day_of(a)));
                            if ui.link(format::opt_km(a.distance())).clicked() {
                                open = Some(a.id);
                            }
                        }
                        None => {
                            for _ in 0..4 {
                                ui.label(weak(ui, format::dash()));
                            }
                        }
                    }
                    ui.end_row();
                }
            });
    }
    if let Some(id) = open {
        app.go(Page::Activity(id));
    }
}
