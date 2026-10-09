//! Strength training (item 19): exercise list, sets × reps × weight per
//! session, history and progress per exercise, volume.

use super::charts::{self, Axis, ChartOpts, Series};
use super::widgets::{self, weak};
use super::{App, Page};
use crate::analysis::load::{day_of, week_start};
use crate::format;
use crate::model::{Activity, Modality};
use crate::planning::{Exercise, SetGroup};
use chrono::Duration;
use eframe::egui::{self, RichText, Ui};

#[derive(Default)]
pub struct State {
    selected: Option<i64>,
    new_name: String,
    new_group: String,
    /// Set groups of the activity being edited, with text buffers.
    editing: Option<(i64, Vec<RowEdit>)>,
}

#[derive(Clone)]
struct RowEdit {
    g: SetGroup,
    sets: String,
    reps: String,
    weight: String,
    rpe: String,
    rest: String,
}

impl RowEdit {
    fn new(g: SetGroup) -> RowEdit {
        RowEdit {
            sets: g.sets.to_string(),
            reps: g.reps.map(|v| v.to_string()).unwrap_or_default(),
            weight: g.weight_kg.map(|v| format::num_trim(v, 2)).unwrap_or_default(),
            rpe: g.rpe.map(|v| format::num_trim(v, 1)).unwrap_or_default(),
            rest: g.rest_s.map(|v| format!("{}", v.round())).unwrap_or_default(),
            g,
        }
    }

    fn apply(&mut self) {
        self.g.sets = self.sets.trim().parse().unwrap_or(1).max(1);
        self.g.reps = self.reps.trim().parse().ok();
        self.g.weight_kg = format::parse_num(&self.weight);
        self.g.rpe = format::parse_num(&self.rpe).filter(|v| (1.0..=10.0).contains(v));
        self.g.rest_s = format::parse_num(&self.rest);
    }
}

/// "3 × 10 × 60 kg"
pub fn describe(g: &SetGroup) -> String {
    let mut s = format!(
        "{} × {}",
        g.sets,
        g.reps.map(|r| r.to_string()).unwrap_or_else(|| "?".into())
    );
    if let Some(w) = g.weight_kg {
        s.push_str(&format!(" × {} kg", format::num_trim(w, 1)));
    }
    s
}

pub fn overview(app: &mut App, ui: &mut Ui) {
    let _ = app.db.ensure_default_exercises();
    let exercises = app.db.exercises().unwrap_or_default();
    if app.pages.strength.selected.is_none() {
        // start with the exercise that has the most history
        app.pages.strength.selected = exercises
            .iter()
            .filter(|e| !e.archived)
            .max_by_key(|e| app.db.exercise_history(e.id).map(|h| h.len()).unwrap_or(0))
            .map(|e| e.id);
    }
    let today = chrono::Local::now().date_naive();
    let sessions: Vec<&Activity> = app
        .data
        .activities
        .iter()
        .filter(|a| a.modality == Modality::Strength)
        .collect();
    let vols = app.db.strength_volumes().unwrap_or_default();
    ui.horizontal(|ui| {
        let last4 = sessions
            .iter()
            .filter(|a| day_of(a) > today - Duration::days(28))
            .count();
        widgets::stat_card(
            ui,
            tr!("Sessões (4 semanas)", "Sessions (4 weeks)"),
            &last4.to_string(),
            Some(&trf!(
                "{} por semana",
                "{} per week",
                format::num(last4 as f64 / 4.0, 1)
            )),
            190.0,
        );
        let vol4: f64 = sessions
            .iter()
            .filter(|a| day_of(a) > today - Duration::days(28))
            .filter_map(|a| vols.get(&a.id))
            .sum();
        widgets::stat_card(
            ui,
            tr!("Volume (4 semanas)", "Volume (4 weeks)"),
            &format!("{} kg", format::int(vol4)),
            Some(tr!("séries × reps × peso", "sets × reps × weight")),
            190.0,
        );
        widgets::stat_card(
            ui,
            tr!("Total de sessões", "Total sessions"),
            &sessions.len().to_string(),
            None,
            190.0,
        );
    });
    // sessions per week, last 12 weeks
    let ws = week_start(today);
    let bars: Vec<_> = (0..12)
        .rev()
        .map(|k| {
            let s = ws - Duration::days(7 * k);
            let n = sessions
                .iter()
                .filter(|a| (s..s + Duration::days(7)).contains(&day_of(a)))
                .count() as f64;
            let v: f64 = sessions
                .iter()
                .filter(|a| (s..s + Duration::days(7)).contains(&day_of(a)))
                .filter_map(|a| vols.get(&a.id))
                .sum();
            (
                charts::date_to_x(s),
                n,
                widgets::modality_color(Modality::Strength),
                format!("{} · {} · {} kg", format::date_short(s), n, format::num(v, 0)),
            )
        })
        .collect();
    let mut o = ChartOpts::new("str-weeks", Axis::Date, Axis::Plain);
    o.title = Some(tr!("Sessões por semana", "Sessions per week"));
    o.height = 120.0;
    charts::bar_chart(ui, o, bars, 5.0);

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(240.0);
            ui.label(RichText::new(tr!("Exercícios", "Exercises")).strong());
            let s = &mut app.pages.strength;
            for e in exercises.iter().filter(|e| !e.archived) {
                if ui
                    .selectable_label(
                        s.selected == Some(e.id),
                        format!(
                            "{}  {}",
                            e.name,
                            if e.muscle_group.is_empty() {
                                String::new()
                            } else {
                                format!("· {}", e.muscle_group)
                            }
                        ),
                    )
                    .clicked()
                {
                    s.selected = Some(e.id);
                }
            }
            let mut add = false;
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut s.new_name)
                        .hint_text(tr!("novo exercício", "new exercise"))
                        .desired_width(110.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut s.new_group)
                        .hint_text(tr!("grupo", "group"))
                        .desired_width(60.0),
                );
                add = ui.button("➕").clicked() && !s.new_name.trim().is_empty();
            });
            if add {
                let e = Exercise {
                    name: s.new_name.trim().into(),
                    muscle_group: s.new_group.trim().into(),
                    ..Default::default()
                };
                match app.db.save_exercise(&e) {
                    Ok(id) => {
                        let s = &mut app.pages.strength;
                        s.selected = Some(id);
                        s.new_name.clear();
                        s.new_group.clear();
                    }
                    Err(_) => app.toast_error(tr!(
                        "Já existe um exercício com esse nome.",
                        "An exercise with this name already exists."
                    )),
                }
            }
        });
        ui.separator();
        ui.vertical(|ui| exercise_history(app, ui, &exercises));
    });
}

fn exercise_history(app: &mut App, ui: &mut Ui, exercises: &[Exercise]) {
    let Some(ex) = exercises.iter().find(|e| Some(e.id) == app.pages.strength.selected) else {
        ui.label(weak(
            ui,
            tr!(
                "Escolha um exercício para ver o histórico e a evolução.",
                "Choose an exercise to see its history and progress."
            ),
        ));
        return;
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(&ex.name).size(17.0).strong());
        if ui
            .small_button(tr!("Arquivar", "Archive"))
            .on_hover_text(tr!(
                "Some da lista; o histórico é mantido",
                "Hidden from the list; history is kept"
            ))
            .clicked()
        {
            let mut e = ex.clone();
            e.archived = true;
            let _ = app.db.save_exercise(&e);
            app.pages.strength.selected = None;
        }
    });
    let hist = app.db.exercise_history(ex.id).unwrap_or_default();
    if hist.is_empty() {
        ui.label(weak(
            ui,
            tr!(
                "Sem registros. Abra uma sessão de musculação e adicione este exercício na aba \"Exercícios\".",
                "No records. Open a strength session and add this exercise in the \"Exercises\" tab."
            ),
        ));
        return;
    }
    // per session: description, max weight, volume
    let mut sessions: Vec<(i64, i64, Vec<SetGroup>)> = Vec::new();
    for (t, g) in hist {
        match sessions.last_mut() {
            Some((_, aid, v)) if *aid == g.activity_id => v.push(g),
            _ => sessions.push((t, g.activity_id, vec![g])),
        }
    }
    let date = |t: i64| {
        chrono::DateTime::from_timestamp(t, 0)
            .map(|d| d.date_naive())
            .unwrap_or_default()
    };
    let weight: Vec<[f64; 2]> = sessions
        .iter()
        .filter_map(|(t, _, v)| {
            Some([
                charts::date_to_x(date(*t)),
                v.iter().filter_map(|g| g.weight_kg).reduce(f64::max)?,
            ])
        })
        .collect();
    let volume: Vec<[f64; 2]> = sessions
        .iter()
        .map(|(t, _, v)| [charts::date_to_x(date(*t)), v.iter().map(|g| g.volume()).sum()])
        .collect();
    ui.columns(2, |c| {
        let mut o = ChartOpts::new("ex-weight", Axis::Date, Axis::Plain);
        o.title = Some(tr!("Maior peso (kg)", "Top weight (kg)"));
        o.height = 160.0;
        charts::line_chart(
            &mut c[0],
            o,
            vec![
                Series::line("kg", widgets::CADENCE, weight.clone()),
                Series::dots("kg", widgets::CADENCE, weight),
            ],
        );
        let mut o = ChartOpts::new("ex-volume", Axis::Date, Axis::Plain);
        o.title = Some(tr!("Volume (kg)", "Volume (kg)"));
        o.height = 160.0;
        charts::line_chart(
            &mut c[1],
            o,
            vec![
                Series::line(tr!("volume", "volume"), widgets::ACCENT, volume.clone()),
                Series::dots(tr!("volume", "volume"), widgets::ACCENT, volume),
            ],
        );
    });
    let mut open = None;
    egui::Grid::new("ex-hist").striped(true).num_columns(4).show(ui, |ui| {
        for (t, aid, v) in sessions.iter().rev() {
            if ui.link(format::date(date(*t))).clicked() {
                open = Some(*aid);
            }
            ui.label(RichText::new(v.iter().map(describe).collect::<Vec<_>>().join(" + ")).strong());
            ui.label(weak(
                ui,
                trf!(
                    "volume {} kg",
                    "volume {} kg",
                    format::num(v.iter().map(|g| g.volume()).sum(), 0)
                ),
            ));
            let rpe: Vec<String> = v.iter().filter_map(|g| g.rpe).map(|r| format::num_trim(r, 1)).collect();
            ui.label(weak(
                ui,
                if rpe.is_empty() {
                    String::new()
                } else {
                    format!("RPE {}", rpe.join("/"))
                },
            ));
            ui.end_row();
        }
    });
    if let Some(id) = open {
        app.pages.activity.open_tab = Some(super::activity::Tab::Exercises);
        app.go(Page::Activity(id));
    }
}

/// "Exercises" tab of a strength activity.
pub fn sets_tab(app: &mut App, ui: &mut Ui, activity_id: i64) {
    let _ = app.db.ensure_default_exercises();
    let exercises: Vec<Exercise> = app
        .db
        .exercises()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| !e.archived)
        .collect();
    if app.pages.strength.editing.as_ref().map(|e| e.0) != Some(activity_id) {
        let rows = app
            .db
            .set_groups(activity_id)
            .unwrap_or_default()
            .into_iter()
            .map(RowEdit::new)
            .collect();
        app.pages.strength.editing = Some((activity_id, rows));
    }
    let Some((_, rows)) = app.pages.strength.editing.as_mut() else {
        return;
    };
    ui.label(weak(
        ui,
        tr!(
            "Uma linha por exercício (ou por variação de carga). Volume = séries × repetições × peso.",
            "One row per exercise (or per load change). Volume = sets × reps × weight."
        ),
    ));
    let mut remove = None;
    let mut dirty = false;
    egui::Grid::new("sets")
        .striped(true)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            for t in [
                tr!("Exercício", "Exercise"),
                tr!("Séries", "Sets"),
                tr!("Reps", "Reps"),
                tr!("Peso (kg)", "Weight (kg)"),
                "RPE",
                tr!("Descanso (s)", "Rest (s)"),
                tr!("Observações", "Notes"),
                tr!("Volume", "Volume"),
                "",
            ] {
                ui.label(RichText::new(t).strong());
            }
            ui.end_row();
            for (i, r) in rows.iter_mut().enumerate() {
                let name = exercises
                    .iter()
                    .find(|e| e.id == r.g.exercise_id)
                    .map(|e| e.name.clone())
                    .unwrap_or_else(|| tr!("escolha…", "choose…").into());
                egui::ComboBox::from_id_salt(format!("ex{i}"))
                    .width(140.0)
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for e in &exercises {
                            dirty |= ui.selectable_value(&mut r.g.exercise_id, e.id, &e.name).changed();
                        }
                    });
                for (buf, w) in [
                    (&mut r.sets, 36.0),
                    (&mut r.reps, 36.0),
                    (&mut r.weight, 54.0),
                    (&mut r.rpe, 36.0),
                    (&mut r.rest, 50.0),
                ] {
                    dirty |= ui.add(egui::TextEdit::singleline(buf).desired_width(w)).changed();
                }
                dirty |= ui
                    .add(egui::TextEdit::singleline(&mut r.g.notes).desired_width(140.0))
                    .changed();
                r.apply();
                ui.label(format!("{} kg", format::num(r.g.volume(), 0)));
                if ui.small_button("✖").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        rows.remove(i);
        dirty = true;
    }
    let total: f64 = rows.iter().map(|r| r.g.volume()).sum();
    ui.horizontal(|ui| {
        if ui.button(tr!("➕ Exercício", "➕ Exercise")).clicked() {
            // repeat the last exercise's numbers to speed up typing
            let mut g = rows.last().map(|r| r.g.clone()).unwrap_or(SetGroup {
                sets: 3,
                reps: Some(10),
                ..Default::default()
            });
            g.id = 0;
            g.activity_id = activity_id;
            if rows.iter().any(|r| r.g.exercise_id == g.exercise_id) {
                g.exercise_id = exercises
                    .iter()
                    .find(|e| !rows.iter().any(|r| r.g.exercise_id == e.id))
                    .map(|e| e.id)
                    .unwrap_or(0);
            }
            rows.push(RowEdit::new(g));
            dirty = true;
        }
        ui.label(
            RichText::new(trf!(
                "Volume total: {} kg",
                "Total volume: {} kg",
                format::num(total, 0)
            ))
            .strong(),
        );
    });
    if dirty {
        let groups: Vec<SetGroup> = rows.iter().map(|r| r.g.clone()).collect();
        if let Err(e) = app.db.save_set_groups(activity_id, &groups) {
            app.toast_error(e.to_string());
        }
    }
}
