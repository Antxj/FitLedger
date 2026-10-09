//! Activity detail (items 5, 6, 7, 9, 12, 26, 38): summary, charts, splits,
//! laps, heart-rate zones, cardiac drift, route, notes and data quality.

use super::charts::{self, Axis, ChartOpts, Series};
use super::widgets::{self, weak};
use super::{App, Page};
use crate::analysis::hr;
use crate::analysis::metrics::{self, Metrics, Terrain};
use crate::analysis::splits::{self, MILE_M};
use crate::analysis::stream::Stream;
use crate::format;
use crate::model::{Activity, Lap, Modality};
use eframe::egui::{self, Color32, RichText, Ui};
use egui_extras::{Column, TableBuilder};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Charts,
    Splits,
    Laps,
    Zones,
    Drift,
    Map,
    Notes,
    Quality,
}

#[derive(Default)]
pub struct State {
    id: i64,
    generation: u64,
    stream: Option<Stream>,
    laps: Vec<Lap>,
    pub tab: Tab,
    x_distance: bool,
    split_step: f64,
    custom_split: String,
    drift_skip_min: f64,
    edit: Option<Edit>,
    confirm_delete: bool,
    ref_tests: Vec<(i64, String, bool)>,
}

/// Text buffers of the notes/edit form.
struct Edit {
    a: Activity,
    distance_km: String,
    incline: String,
    weight: String,
}

impl Edit {
    fn new(a: &Activity) -> Edit {
        Edit {
            a: a.clone(),
            distance_km: a.distance_override_m.map(|d| format::num_trim(d / 1000.0, 3)).unwrap_or_default(),
            incline: a.incline_pct.map(|v| format::num_trim(v, 1)).unwrap_or_default(),
            weight: a.body_weight.map(|v| format::num_trim(v, 1)).unwrap_or_default(),
        }
    }
}

fn load(app: &mut App, id: i64) {
    let st = &mut app.pages.activity;
    if st.id == id && st.generation == app.data.generation && st.stream.is_some() {
        return;
    }
    let keep_tab = st.id == id;
    let Some(a) = app.data.get(id).cloned() else { return };
    let stream = metrics::stream_for(&app.db, &a).unwrap_or_default();
    let laps = app.db.laps(id).unwrap_or_default();
    let ref_tests = app.db.reference_tests_for_activity(id).unwrap_or_default();
    let st = &mut app.pages.activity;
    st.id = id;
    st.generation = app.data.generation;
    st.stream = Some(stream);
    st.laps = laps;
    st.ref_tests = ref_tests;
    st.edit = Some(Edit::new(&a));
    if !keep_tab {
        st.tab = Tab::Charts;
        st.split_step = 1000.0;
        st.confirm_delete = false;
    }
    if st.split_step == 0.0 {
        st.split_step = 1000.0;
    }
}

pub fn show(app: &mut App, ui: &mut Ui, id: i64) {
    load(app, id);
    let Some(a) = app.data.get(id).cloned() else {
        widgets::empty_state(ui, tr!("Atividade não encontrada.", "Activity not found."));
        if ui.button(tr!("← Voltar", "← Back")).clicked() {
            app.back();
        }
        return;
    };
    let m = app.data.metrics(id).cloned().unwrap_or_default();

    header(app, ui, &a);
    summary_cards(ui, &a, &m);
    ui.add_space(6.0);

    let has_stream = app.pages.activity.stream.as_ref().is_some_and(|s| !s.is_empty());
    let run = a.modality.is_run();
    let has_gps = a.has_gps;
    let mut tabs: Vec<(Tab, &str)> = Vec::new();
    if has_stream {
        tabs.push((Tab::Charts, tr!("Gráficos", "Charts")));
    }
    if run && has_stream {
        tabs.push((Tab::Splits, "Splits"));
    }
    if !app.pages.activity.laps.is_empty() {
        tabs.push((Tab::Laps, tr!("Voltas", "Laps")));
    }
    if a.avg_hr.is_some() {
        tabs.push((Tab::Zones, tr!("Zonas de FC", "HR zones")));
    }
    if run && has_stream {
        tabs.push((Tab::Drift, tr!("Deriva cardíaca", "Cardiac drift")));
    }
    if has_gps {
        tabs.push((Tab::Map, tr!("Percurso", "Route")));
    }
    tabs.push((Tab::Notes, tr!("Notas e ajustes", "Notes and edits")));
    if has_stream {
        tabs.push((Tab::Quality, tr!("Qualidade dos dados", "Data quality")));
    }
    if !tabs.iter().any(|(t, _)| *t == app.pages.activity.tab) {
        app.pages.activity.tab = tabs[0].0;
    }
    ui.horizontal(|ui| {
        for (t, label) in &tabs {
            ui.selectable_value(&mut app.pages.activity.tab, *t, *label);
        }
    });
    ui.separator();

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match app.pages.activity.tab {
        Tab::Charts => charts_tab(app, ui, &a),
        Tab::Splits => splits_tab(app, ui),
        Tab::Laps => laps_tab(app, ui, &a),
        Tab::Zones => zones_tab(ui, &m),
        Tab::Drift => drift_tab(app, ui),
        Tab::Map => map_tab(app, ui),
        Tab::Notes => notes_tab(app, ui, &a),
        Tab::Quality => quality_tab(ui, &m),
    });

    let ctx = ui.ctx().clone();
    if widgets::confirm_window(
        &ctx,
        &mut app.pages.activity.confirm_delete,
        tr!("Excluir atividade", "Delete activity"),
        tr!(
            "Excluir esta atividade do FitLedger? O arquivo .fit original na pasta /fits não é apagado \
             (ele será importado de novo na próxima vez, a menos que você o remova da pasta).",
            "Delete this activity from FitLedger? The original .fit file in the /fits folder is not \
             deleted (it will be imported again next time unless you remove it from the folder)."
        ),
    ) {
        match app.db.delete_activity(a.id) {
            Ok(()) => {
                app.reload();
                app.toast(tr!("Atividade excluída.", "Activity deleted."));
                app.back();
            }
            Err(e) => app.toast_error(e.to_string()),
        }
    }
}

fn header(app: &mut App, ui: &mut Ui, a: &Activity) {
    ui.horizontal(|ui| {
        if ui.button(tr!("← Voltar", "← Back")).clicked() {
            app.back();
        }
        ui.label(RichText::new(a.title()).size(22.0).strong());
        widgets::modality_chip(ui, a.modality);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(RichText::new(tr!("🗑 Excluir", "🗑 Delete")).color(widgets::BAD)).clicked() {
                app.pages.activity.confirm_delete = true;
            }
            if a.source == "manual" && ui.button(tr!("✏ Editar", "✏ Edit")).clicked() {
                app.pages.manual = super::manual::State::default();
                app.go(Page::Manual(Some(a.id)));
            }
            if ui.button(tr!("⚖ Comparar com…", "⚖ Compare with…")).clicked() {
                if !app.pages.activities.selected.contains(&a.id) {
                    app.pages.activities.selected.push(a.id);
                }
                app.toast(tr!(
                    "Marque as outras atividades na lista e clique em Comparar.",
                    "Tick the other activities in the list and click Compare."
                ));
                app.go(Page::Activities(if a.modality.is_run() { None } else { Some(a.modality) }));
            }
        });
    });
    let mut info = format::datetime(a.local_start());
    if let Some(d) = &a.device {
        info.push_str(&format!(" · {d}"));
    }
    if a.source == "manual" {
        info.push_str(tr!(" · cadastro manual", " · manual entry"));
    }
    ui.label(weak(ui, info));
    ui.add_space(4.0);
}

fn summary_cards(ui: &mut Ui, a: &Activity, m: &Metrics) {
    let mut cards: Vec<(String, String, Option<String>)> = Vec::new();
    if let Some(d) = a.distance() {
        let sub = a.distance_override_m.map(|_| tr!("corrigida manualmente", "manually corrected").to_string());
        cards.push((tr!("Distância", "Distance").into(), format::km(d), sub));
    }
    cards.push((
        tr!("Duração", "Duration").into(),
        format::opt_duration(a.elapsed_s.or(a.timer_s)),
        a.moving_s.map(|mv| trf!("em movimento {}", "moving {}", format::duration(mv))),
    ));
    if a.modality.is_run() {
        let kmh = a.pace_s_per_km().map(|p| format!("{} km/h", format::num(format::pace_to_kmh(p), 1)));
        cards.push((tr!("Pace médio", "Avg pace").into(), format::opt_pace(a.pace_s_per_km()), kmh));
    }
    if a.avg_hr.is_some() {
        cards.push((
            tr!("FC média", "Avg HR").into(),
            format::bpm(a.avg_hr),
            Some(trf!("máx {}", "max {}", format::bpm(a.max_hr))),
        ));
    }
    if let Some(c) = a.avg_cadence {
        cards.push((
            tr!("Cadência", "Cadence").into(),
            format!("{} spm", c.round()),
            a.max_cadence.map(|v| trf!("máx {}", "max {}", v.round())),
        ));
    }
    if let Some(c) = a.calories {
        cards.push((tr!("Calorias", "Calories").into(), format!("{} kcal", c.round()), None));
    }
    let gain = a.ascent_m.or(m.gain_m);
    if let Some(g) = gain {
        let loss = a.descent_m.or(m.loss_m);
        cards.push((
            tr!("Elevação", "Elevation").into(),
            format!("+{} m", g.round()),
            loss.map(|l| format!("-{} m · {}", l.round(), Terrain::of(a, Some(m)).label())),
        ));
    }
    if a.modality == Modality::Treadmill {
        cards.push((
            tr!("Inclinação", "Incline").into(),
            a.incline_pct.map(|v| format!("{}%", format::num_trim(v, 1))).unwrap_or_else(format::dash),
            Some(tr!("editável em Notas", "editable in Notes").into()),
        ));
    }
    if m.load > 0.0 {
        let src = match m.load_source.as_str() {
            "hr" => tr!("pela FC", "from HR"),
            "rpe" => tr!("pelo RPE", "from RPE"),
            _ => tr!("estimada", "estimated"),
        };
        cards.push((tr!("Carga", "Load").into(), format!("{}", m.load.round()), Some(src.into())));
    }
    if let Some(p) = a.avg_power {
        cards.push((tr!("Potência", "Power").into(), format!("{} W", p.round()), a.max_power.map(|v| trf!("máx {} W", "max {} W", v.round()))));
    }
    if let Some(t) = a.avg_temp {
        cards.push((tr!("Temperatura", "Temperature").into(), format!("{} °C", format::num(t, 0)), None));
    }
    if let Some(te) = a.training_effect {
        cards.push((
            tr!("Efeito de treino", "Training effect").into(),
            format::num(te, 1),
            a.anaerobic_te.map(|v| trf!("anaeróbico {}", "anaerobic {}", format::num(v, 1))),
        ));
    }
    widgets::card_grid(ui, 150.0, &cards);
}

/// (x, y) points of a series, x in seconds or meters.
fn xy(st: &Stream, x_distance: bool, y: impl Fn(usize) -> Option<f64>) -> Vec<[f64; 2]> {
    (0..st.len())
        .filter_map(|i| {
            let x = if x_distance { st.dist[i]? } else { st.t[i] };
            Some([x, y(i)?])
        })
        .collect()
}

fn charts_tab(app: &mut App, ui: &mut Ui, a: &Activity) {
    let st_ref = &mut app.pages.activity;
    let has_dist = st_ref.stream.as_ref().is_some_and(|s| s.dist.iter().any(Option::is_some));
    ui.horizontal(|ui| {
        ui.label(tr!("Eixo X:", "X axis:"));
        ui.selectable_value(&mut st_ref.x_distance, false, tr!("Tempo", "Time"));
        if has_dist {
            ui.selectable_value(&mut st_ref.x_distance, true, tr!("Distância", "Distance"));
        }
        ui.label(weak(ui, tr!(
            "Passe o mouse para ver os valores. Arraste para mover e use a roda do mouse com Ctrl para zoom; duplo clique volta ao normal.",
            "Hover to see the values. Drag to pan and Ctrl + mouse wheel to zoom; double-click resets."
        )).small());
    });
    if !has_dist {
        st_ref.x_distance = false;
    }
    let xd = st_ref.x_distance;
    let Some(st) = st_ref.stream.as_ref() else { return };
    let xa = if xd { Axis::Distance } else { Axis::Time };
    let group = format!("act-{}", a.id);
    let mk = |id: &'static str, y: Axis, title: &'static str| {
        let mut o = ChartOpts::new(id, xa, y);
        o.link = Some(group.as_str());
        o.title = Some(title);
        o.height = 170.0;
        o
    };

    if st.hr.iter().any(Option::is_some) {
        let pts = xy(st, xd, |i| st.hr[i]);
        let mut o = mk("hr", Axis::Bpm, tr!("Frequência cardíaca", "Heart rate"));
        if let Some(avg) = a.avg_hr {
            o.hlines.push((avg, tr!("média", "average").into()));
        }
        charts::line_chart(ui, o, vec![Series::line(tr!("FC", "HR"), widgets::HR, pts)]);
    }
    if a.modality.is_run() && st.speed.iter().any(Option::is_some) {
        let speed = xy(st, xd, |i| st.speed[i].filter(|_| st.moving[i]));
        let smooth = charts::smooth(&speed, 7);
        let pace: Vec<[f64; 2]> = smooth
            .iter()
            .filter(|p| p[1] > 0.8)
            .map(|p| [p[0], (1000.0 / p[1]).min(900.0)])
            .collect();
        let mut o = mk("pace", Axis::Pace, tr!("Pace (suavizado em 15 s)", "Pace (15 s smoothing)"));
        o.invert_y = true;
        if let Some(p) = a.pace_s_per_km() {
            o.hlines.push((p, tr!("média", "average").into()));
        }
        charts::line_chart(ui, o, vec![Series::line("Pace", widgets::PACE, pace)]);
        let kmh: Vec<[f64; 2]> = smooth.iter().map(|p| [p[0], p[1] * 3.6]).collect();
        charts::line_chart(
            ui,
            mk("speed", Axis::SpeedKmh, tr!("Velocidade", "Speed")),
            vec![Series::line(tr!("Velocidade", "Speed"), widgets::SPEED, kmh)],
        );
    }
    if st.cadence.iter().any(Option::is_some) {
        let pts = xy(st, xd, |i| st.cadence[i]);
        charts::line_chart(
            ui,
            mk("cad", Axis::Cadence, tr!("Cadência", "Cadence")),
            vec![Series::dots(tr!("Cadência", "Cadence"), widgets::CADENCE, charts::downsample(&pts, 900))],
        );
    }
    if st.alt.iter().any(Option::is_some) && a.modality != Modality::Treadmill {
        let pts = xy(st, xd, |i| st.alt[i]);
        charts::line_chart(
            ui,
            mk("alt", Axis::Meters, tr!("Altitude", "Altitude")),
            vec![Series::line(tr!("Altitude", "Altitude"), widgets::ALT, pts)],
        );
    }
    if st.power.iter().any(Option::is_some) {
        let pts = charts::smooth(&xy(st, xd, |i| st.power[i]), 5);
        charts::line_chart(
            ui,
            mk("pow", Axis::Watts, tr!("Potência", "Power")),
            vec![Series::line(tr!("Potência", "Power"), widgets::POWER, pts)],
        );
    }
    if st.temp.iter().any(Option::is_some) {
        let pts = xy(st, xd, |i| st.temp[i]);
        charts::line_chart(
            ui,
            mk("temp", Axis::Plain, tr!("Temperatura (°C)", "Temperature (°C)")),
            vec![Series::line("°C", Color32::from_rgb(0xf4, 0x72, 0xb6), pts)],
        );
    }
    let pauses = st.pauses();
    if !pauses.is_empty() {
        widgets::section(ui, tr!("Pausas", "Pauses"));
        for (start, dur) in pauses {
            ui.label(trf!(
                "• aos {}: parado por {}",
                "• at {}: stopped for {}",
                format::duration(start),
                format::duration(dur)
            ));
        }
    }
}

fn splits_tab(app: &mut App, ui: &mut Ui) {
    let s = &mut app.pages.activity;
    ui.horizontal(|ui| {
        ui.label(tr!("Dividir a cada:", "Split every:"));
        for (v, label) in [(500.0, "500 m"), (1000.0, "1 km"), (MILE_M, tr!("1 milha", "1 mile"))] {
            if ui.selectable_label(s.split_step == v, label).clicked() {
                s.split_step = v;
                s.custom_split.clear();
            }
        }
        ui.label(tr!("ou", "or"));
        let r = ui.add(egui::TextEdit::singleline(&mut s.custom_split).hint_text(tr!("metros", "meters")).desired_width(70.0));
        if r.changed() {
            if let Some(v) = format::parse_num(&s.custom_split).filter(|v| *v >= 100.0) {
                s.split_step = v;
            }
        }
        ui.label("m");
    });
    let Some(st) = s.stream.as_ref() else { return };
    let rows = splits::splits(st, s.split_step);
    if rows.is_empty() {
        widgets::empty_state(ui, tr!("Sem dados de distância para calcular splits.", "No distance data to compute splits."));
        return;
    }
    let best = rows.iter().filter(|r| r.distance_m >= s.split_step * 0.9).map(|r| r.pace_s_km).fold(f64::MAX, f64::min);
    let unit_km = s.split_step / 1000.0;
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .columns(Column::initial(90.0).at_least(60.0), 8)
        .header(24.0, |mut h| {
            for t in [
                if (unit_km - 1.0).abs() < 1e-9 { "KM" } else { "#" },
                tr!("Distância", "Distance"),
                tr!("Tempo", "Time"),
                "Pace",
                tr!("FC média", "Avg HR"),
                tr!("FC máxima", "Max HR"),
                tr!("Cadência", "Cadence"),
                tr!("Elevação", "Elevation"),
            ] {
                h.col(|ui| {
                    ui.label(RichText::new(t).strong());
                });
            }
        })
        .body(|mut body| {
            for r in &rows {
                body.row(24.0, |mut row| {
                    row.col(|ui| {
                        ui.label(r.n.to_string());
                    });
                    row.col(|ui| {
                        ui.label(format::km(r.distance_m));
                    });
                    row.col(|ui| {
                        ui.label(format::duration(r.time_s));
                    });
                    row.col(|ui| {
                        let t = RichText::new(format::pace_unit(r.pace_s_km));
                        ui.label(if (r.pace_s_km - best).abs() < 0.5 { t.strong().color(widgets::GOOD) } else { t });
                    });
                    row.col(|ui| {
                        ui.label(format::bpm(r.avg_hr));
                    });
                    row.col(|ui| {
                        ui.label(format::bpm(r.max_hr));
                    });
                    row.col(|ui| {
                        ui.label(r.avg_cadence.map(|c| format!("{} spm", c.round())).unwrap_or_else(format::dash));
                    });
                    row.col(|ui| {
                        ui.label(format!("+{} / -{} m", r.gain_m.round() as i64, r.loss_m.round() as i64));
                    });
                });
            }
        });
    ui.add_space(8.0);
    let pace: Vec<[f64; 2]> = rows.iter().map(|r| [r.n as f64, r.pace_s_km]).collect();
    let hr: Vec<[f64; 2]> = rows.iter().filter_map(|r| Some([r.n as f64, r.avg_hr?])).collect();
    ui.columns(2, |c| {
        let mut o = ChartOpts::new("splits-pace", Axis::Plain, Axis::Pace);
        o.title = Some(tr!("Pace por split", "Pace per split"));
        o.height = 170.0;
        o.invert_y = true;
        charts::line_chart(&mut c[0], o, vec![Series::line("Pace", widgets::PACE, pace.clone()), Series::dots("Pace", widgets::PACE, pace)]);
        let mut o = ChartOpts::new("splits-hr", Axis::Plain, Axis::Bpm);
        o.title = Some(tr!("FC média por split", "Avg HR per split"));
        o.height = 170.0;
        charts::line_chart(&mut c[1], o, vec![Series::line(tr!("FC", "HR"), widgets::HR, hr.clone()), Series::dots(tr!("FC", "HR"), widgets::HR, hr)]);
    });
}

fn laps_tab(app: &mut App, ui: &mut Ui, a: &Activity) {
    let laps = &app.pages.activity.laps;
    ui.label(weak(ui, tr!("Voltas como gravadas pelo relógio.", "Laps as recorded by the watch.")));
    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .columns(Column::initial(95.0).at_least(60.0), 8)
        .header(24.0, |mut h| {
            for t in [
                "#",
                tr!("Distância", "Distance"),
                tr!("Tempo", "Time"),
                "Pace",
                tr!("FC média", "Avg HR"),
                tr!("FC máxima", "Max HR"),
                tr!("Cadência", "Cadence"),
                tr!("Elevação", "Elevation"),
            ] {
                h.col(|ui| {
                    ui.label(RichText::new(t).strong());
                });
            }
        })
        .body(|mut body| {
            for (i, l) in laps.iter().enumerate() {
                body.row(24.0, |mut row| {
                    let time = l.timer_s.or(l.elapsed_s);
                    let pace = match (time, l.distance_m) {
                        (Some(t), Some(d)) if d > 10.0 && a.modality.is_run() => Some(t / (d / 1000.0)),
                        _ => None,
                    };
                    row.col(|ui| {
                        ui.label((i + 1).to_string());
                    });
                    row.col(|ui| {
                        ui.label(format::opt_km(l.distance_m));
                    });
                    row.col(|ui| {
                        ui.label(format::opt_duration(time));
                    });
                    row.col(|ui| {
                        ui.label(format::opt_pace(pace));
                    });
                    row.col(|ui| {
                        ui.label(format::bpm(l.avg_hr));
                    });
                    row.col(|ui| {
                        ui.label(format::bpm(l.max_hr));
                    });
                    row.col(|ui| {
                        ui.label(l.avg_cadence.map(|c| format!("{} spm", c.round())).unwrap_or_else(format::dash));
                    });
                    row.col(|ui| {
                        ui.label(match (l.ascent_m, l.descent_m) {
                            (None, None) => format::dash(),
                            (u, d) => format!("+{} / -{} m", u.unwrap_or(0.0).round(), d.unwrap_or(0.0).round()),
                        });
                    });
                });
            }
        });
}

pub fn zones_table(ui: &mut Ui, m: &Metrics) {
    let total: f64 = m.zones_s.iter().sum();
    if total <= 0.0 {
        widgets::empty_state(ui, tr!("Sem dados de frequência cardíaca.", "No heart-rate data."));
        return;
    }
    let zones = crate::settings::Zones { lower: m.zone_lower, max: m.zone_max };
    let names = [
        tr!("Abaixo da Z1", "Below Z1"),
        tr!("Z1 · Recuperação", "Z1 · Recovery"),
        tr!("Z2 · Aeróbico leve", "Z2 · Easy aerobic"),
        tr!("Z3 · Moderado", "Z3 · Moderate"),
        tr!("Z4 · Limiar", "Z4 · Threshold"),
        tr!("Z5 · Máximo", "Z5 · Maximum"),
    ];
    for z in (0..6).rev() {
        let t = m.zones_s[z];
        if z == 0 && t <= 0.0 {
            continue;
        }
        let range = if z == 0 {
            format!("< {}", zones.lower[0].round())
        } else {
            let (lo, hi) = zones.range(z);
            format!("{}–{} bpm", lo.round(), hi.round())
        };
        ui.horizontal(|ui| {
            ui.add_sized([150.0, 20.0], egui::Label::new(RichText::new(names[z]).color(widgets::ZONES[z])));
            ui.add_sized([95.0, 20.0], egui::Label::new(weak(ui, range)));
            let frac = (t / total) as f32;
            ui.add(
                egui::ProgressBar::new(frac)
                    .fill(widgets::ZONES[z])
                    .desired_width((ui.available_width() - 150.0).max(80.0))
                    .text(format!("{}%", format::num(frac as f64 * 100.0, 0))),
            );
            ui.label(format::duration(t));
        });
    }
}

fn zones_tab(ui: &mut Ui, m: &Metrics) {
    ui.label(weak(
        ui,
        trf!(
            "Zonas calculadas com FC máxima de {} bpm. Ajuste em Configurações → Frequência cardíaca.",
            "Zones computed with a max heart rate of {} bpm. Change it in Settings → Heart rate.",
            m.zone_max.round()
        ),
    ));
    ui.add_space(6.0);
    zones_table(ui, m);
}

fn drift_tab(app: &mut App, ui: &mut Ui) {
    let s = &mut app.pages.activity;
    ui.label(weak(
        ui,
        tr!(
            "Compara a primeira e a segunda metade do tempo em movimento. Se a FC sobe mantendo o mesmo pace \
             (desacoplamento positivo), o esforço aeróbico ficou mais caro no fim. Valores abaixo de ~5% \
             costumam indicar boa base aeróbica.",
            "Compares the first and second half of the moving time. If HR rises at the same pace (positive \
             decoupling), the aerobic effort got more expensive at the end. Values below ~5% usually \
             indicate a good aerobic base."
        ),
    ));
    ui.horizontal(|ui| {
        ui.label(tr!("Ignorar aquecimento:", "Skip warm-up:"));
        ui.add(egui::Slider::new(&mut s.drift_skip_min, 0.0..=20.0).step_by(1.0).fixed_decimals(0).suffix(" min"));
    });
    let Some(st) = s.stream.as_ref() else { return };
    let Some(d) = hr::cardiac_drift(st, s.drift_skip_min * 60.0) else {
        widgets::empty_state(
            ui,
            tr!(
                "É preciso pelo menos 10 minutos de corrida com FC (depois do aquecimento ignorado).",
                "At least 10 minutes of running with HR are needed (after the skipped warm-up)."
            ),
        );
        return;
    };
    drift_table(ui, &d);
}

pub fn drift_table(ui: &mut Ui, d: &hr::Drift) {
    let pace = |s: Option<f64>| s.filter(|v| *v > 0.3).map(|v| format::pace_unit(1000.0 / v)).unwrap_or_else(format::dash);
    egui::Grid::new("drift").striped(true).min_col_width(140.0).show(ui, |ui| {
        ui.label("");
        ui.label(RichText::new(tr!("1ª metade", "1st half")).strong());
        ui.label(RichText::new(tr!("2ª metade", "2nd half")).strong());
        ui.label(RichText::new(tr!("Diferença", "Difference")).strong());
        ui.end_row();
        ui.label(tr!("FC média", "Avg HR"));
        ui.label(format::bpm(Some(d.hr1)));
        ui.label(format::bpm(Some(d.hr2)));
        ui.label(format!("{} ({})", format::bpm(Some(d.hr2 - d.hr1)), format::pct_signed(d.hr_pct)));
        ui.end_row();
        ui.label("Pace");
        ui.label(pace(d.speed1));
        ui.label(pace(d.speed2));
        let pace_diff = match (d.speed1, d.speed2) {
            (Some(a), Some(b)) if a > 0.3 && b > 0.3 => {
                let p1 = 1000.0 / a;
                let p2 = 1000.0 / b;
                let sign = if p2 > p1 { "+" } else { "-" };
                format!("{sign}{} s/km ({})", (p2 - p1).abs().round(), format::pct_signed((p2 - p1) / p1 * 100.0))
            }
            _ => format::dash(),
        };
        ui.label(pace_diff);
        ui.end_row();
        ui.label(RichText::new(tr!("Desacoplamento Pa:FC", "Pa:HR decoupling")).strong());
        ui.label("");
        ui.label("");
        let dec = d.decoupling_pct;
        let color = match dec {
            Some(v) if v < 5.0 => widgets::GOOD,
            Some(v) if v < 10.0 => widgets::WARN,
            _ => widgets::BAD,
        };
        ui.label(RichText::new(dec.map(|v| format!("{}%", format::num(v, 1))).unwrap_or_else(format::dash)).strong().color(color));
        ui.end_row();
    });
}

fn map_tab(app: &mut App, ui: &mut Ui) {
    let Some(st) = app.pages.activity.stream.as_ref() else { return };
    let pts: Vec<(usize, f64, f64)> = st.pos.iter().enumerate().filter_map(|(i, p)| p.map(|(la, lo)| (i, la, lo))).collect();
    if pts.len() < 2 {
        widgets::empty_state(ui, tr!("Sem GPS.", "No GPS."));
        return;
    }
    ui.label(weak(
        ui,
        tr!(
            "Traçado desenhado a partir do GPS, sem mapa de fundo (nada é baixado da internet). Passe o mouse sobre a linha.",
            "Route drawn from GPS, with no background map (nothing is downloaded). Hover over the line."
        ),
    ));
    let (mut la0, mut la1, mut lo0, mut lo1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &(_, la, lo) in &pts {
        la0 = la0.min(la);
        la1 = la1.max(la);
        lo0 = lo0.min(lo);
        lo1 = lo1.max(lo);
    }
    let kx = ((la0 + la1) / 2.0).to_radians().cos();
    let w_deg = ((lo1 - lo0) * kx).max(1e-6);
    let h_deg = (la1 - la0).max(1e-6);
    let size = egui::vec2(ui.available_width().min(900.0), 480.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 8.0, ui.visuals().extreme_bg_color);
    let scale = ((rect.width() - 40.0) as f64 / w_deg).min((rect.height() - 40.0) as f64 / h_deg);
    let cx = rect.center().x as f64;
    let cy = rect.center().y as f64;
    let mlo = (lo0 + lo1) / 2.0;
    let mla = (la0 + la1) / 2.0;
    let to_screen = |la: f64, lo: f64| egui::pos2((cx + (lo - mlo) * kx * scale) as f32, (cy - (la - mla) * scale) as f32);
    let screen: Vec<egui::Pos2> = pts.iter().map(|&(_, la, lo)| to_screen(la, lo)).collect();
    painter.add(egui::Shape::line(screen.clone(), egui::Stroke::new(3.0, widgets::ACCENT)));
    // km markers
    let mut next_km = 1000.0;
    for (k, &(i, _, _)) in pts.iter().enumerate() {
        if let Some(d) = st.dist[i] {
            if d >= next_km {
                painter.circle_filled(screen[k], 4.0, Color32::WHITE);
                painter.text(
                    screen[k] + egui::vec2(6.0, -6.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("{}", (next_km / 1000.0) as i64),
                    egui::FontId::proportional(12.0),
                    ui.visuals().text_color(),
                );
                next_km += 1000.0;
            }
        }
    }
    painter.circle_filled(screen[0], 7.0, widgets::GOOD);
    painter.circle_filled(*screen.last().unwrap(), 7.0, widgets::BAD);
    if let Some(hp) = resp.hover_pos() {
        if let Some((k, p)) = screen
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.distance_sq(hp).partial_cmp(&b.1.distance_sq(hp)).unwrap_or(std::cmp::Ordering::Equal))
        {
            if p.distance(hp) < 30.0 {
                let i = pts[k].0;
                painter.circle_stroke(*p, 7.0, egui::Stroke::new(2.0, Color32::WHITE));
                let mut text = format::duration(st.t[i]);
                if let Some(d) = st.dist[i] {
                    text.push_str(&format!(" · {}", format::km(d)));
                }
                if let Some(h) = st.hr[i] {
                    text.push_str(&format!(" · {}", format::bpm(Some(h))));
                }
                if let Some(v) = st.speed[i].filter(|v| *v > 0.5) {
                    text.push_str(&format!(" · {}", format::pace_unit(1000.0 / v)));
                }
                egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::new("map-tip"), egui::PopupAnchor::Pointer)
                    .show(|ui| {
                        ui.label(text);
                    });
            }
        }
    }
    ui.horizontal(|ui| {
        widgets::chip(ui, tr!("● início", "● start"), widgets::GOOD);
        widgets::chip(ui, tr!("● fim", "● finish"), widgets::BAD);
        widgets::chip(ui, tr!("○ km", "○ km"), Color32::GRAY);
    });
}

fn notes_tab(app: &mut App, ui: &mut Ui, a: &Activity) {
    let tests = app.pages.activity.ref_tests.clone();
    let Some(e) = app.pages.activity.edit.as_mut() else { return };
    let mut save = false;
    egui::Grid::new("notes").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(tr!("Título", "Title"));
        let mut name = e.a.name.clone().unwrap_or_default();
        if ui.add(egui::TextEdit::singleline(&mut name).desired_width(320.0)).changed() {
            e.a.name = Some(name).filter(|s| !s.trim().is_empty());
        }
        ui.end_row();
        ui.label(tr!("Modalidade", "Type"));
        egui::ComboBox::from_id_salt("modality").selected_text(e.a.modality.label()).show_ui(ui, |ui| {
            for m in Modality::ALL {
                ui.selectable_value(&mut e.a.modality, m, m.label());
            }
        });
        ui.end_row();
        if e.a.modality == Modality::Treadmill {
            ui.label(tr!("Inclinação da esteira (%)", "Treadmill incline (%)"));
            widgets::opt_number_edit(ui, &mut e.a.incline_pct, &mut e.incline, 80.0);
            ui.end_row();
            ui.label(tr!("Distância corrigida (km)", "Corrected distance (km)"));
            ui.horizontal(|ui| {
                let mut km = e.a.distance_override_m.map(|d| d / 1000.0);
                if widgets::opt_number_edit(ui, &mut km, &mut e.distance_km, 80.0) {
                    e.a.distance_override_m = km.filter(|v| *v > 0.0).map(|v| v * 1000.0);
                }
                ui.label(weak(
                    ui,
                    trf!(
                        "relógio: {} · use o valor do painel da esteira",
                        "watch: {} · use the treadmill display value",
                        format::opt_km(a.distance_m)
                    ),
                ));
            });
            ui.end_row();
        }
        ui.label(tr!("Observações", "Notes"));
        let mut notes = e.a.notes.clone().unwrap_or_default();
        if ui
            .add(
                egui::TextEdit::multiline(&mut notes)
                    .desired_rows(4)
                    .desired_width(420.0)
                    .hint_text(tr!("Ex.: Estava muito calor. Dormi mal. Joguei tênis no dia anterior.", "E.g.: It was very hot. Slept badly. Played tennis the day before.")),
            )
            .changed()
        {
            e.a.notes = Some(notes).filter(|s| !s.trim().is_empty());
        }
        ui.end_row();
        let scale = |ui: &mut Ui, v: &mut Option<i64>, max: i64| {
            ui.horizontal(|ui| {
                if ui.selectable_label(v.is_none(), "—").clicked() {
                    *v = None;
                }
                for i in 1..=max {
                    if ui.selectable_label(*v == Some(i), i.to_string()).clicked() {
                        *v = Some(i);
                    }
                }
            });
        };
        ui.label(tr!("Esforço percebido (RPE 1–10)", "Perceived effort (RPE 1–10)"));
        scale(ui, &mut e.a.rpe, 10);
        ui.end_row();
        ui.label(tr!("Qualidade do sono (1–5)", "Sleep quality (1–5)"));
        scale(ui, &mut e.a.sleep_quality, 5);
        ui.end_row();
        ui.label(tr!("Fadiga (1–5)", "Fatigue (1–5)"));
        scale(ui, &mut e.a.fatigue, 5);
        ui.end_row();
        ui.label(tr!("Dor muscular (1–5)", "Muscle soreness (1–5)"));
        scale(ui, &mut e.a.soreness, 5);
        ui.end_row();
        ui.label(tr!("Peso corporal (kg)", "Body weight (kg)"));
        widgets::opt_number_edit(ui, &mut e.a.body_weight, &mut e.weight, 80.0);
        ui.end_row();
    });
    ui.add_space(8.0);
    if ui.button(RichText::new(tr!("💾 Salvar", "💾 Save")).strong()).clicked() {
        save = true;
    }

    if a.modality.is_run() {
        widgets::section(ui, tr!("Teste de referência", "Reference test"));
        if tests.is_empty() {
            ui.label(weak(ui, tr!(
                "Nenhum teste criado. Crie em Análises → Testes de referência.",
                "No test created yet. Create one in Analysis → Reference tests."
            )));
        }
        for (tid, name, on) in &tests {
            let mut v = *on;
            if ui.checkbox(&mut v, trf!("Esta corrida é uma execução de \"{name}\"", "This run is an execution of \"{name}\"")).changed() {
                if let Err(err) = app.db.set_reference_run(*tid, a.id, v) {
                    app.toast_error(err.to_string());
                }
                app.pages.activity.generation = 0;
            }
        }
    }

    if save {
        let edited = app.pages.activity.edit.as_ref().map(|e| e.a.clone());
        if let Some(edited) = edited {
            match app.db.update_activity_user_fields(&edited) {
                Ok(()) => {
                    let s = app.settings.clone();
                    let obs = app.db.observed_max_hr();
                    let _ = metrics::update_activity(&app.db, &edited, &s, obs);
                    app.reload();
                    app.toast(tr!("Salvo.", "Saved."));
                }
                Err(err) => app.toast_error(err.to_string()),
            }
        }
    }
}

fn quality_tab(ui: &mut Ui, m: &Metrics) {
    let q = &m.quality;
    ui.label(weak(
        ui,
        tr!(
            "Leituras suspeitas são marcadas e ignoradas nos cálculos. O dado original continua guardado sem alteração.",
            "Suspicious readings are flagged and ignored in the calculations. The original data stays stored unchanged."
        ),
    ));
    ui.add_space(6.0);
    egui::Grid::new("quality").striped(true).min_col_width(200.0).show(ui, |ui| {
        let rows = [
            (tr!("Frequência cardíaca (zero, absurda ou pico isolado)", "Heart rate (zero, absurd or isolated spike)"), q.hr),
            (tr!("Velocidade impossível / pace instantâneo anormal", "Impossible speed / abnormal instant pace"), q.speed),
            (tr!("Saltos de GPS", "GPS jumps"), q.gps),
            (tr!("Cadência isolada incorreta", "Isolated wrong cadence"), q.cadence),
            (tr!("Distância voltando para trás", "Distance going backwards"), q.distance),
        ];
        for (label, n) in rows {
            ui.label(label);
            ui.label(RichText::new(n.to_string()).color(if n == 0 { widgets::GOOD } else { widgets::WARN }));
            ui.end_row();
        }
    });
    if q.total() == 0 {
        ui.add_space(6.0);
        widgets::note(ui, tr!("Nenhuma leitura suspeita encontrada.", "No suspicious readings found."), widgets::GOOD);
    }
}
