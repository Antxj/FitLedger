//! Manual activity (item 30), mainly for strength sessions without a FIT file.

use super::widgets::{self, weak};
use super::{App, Page};
use crate::analysis::metrics;
use crate::format;
use crate::model::{Activity, Modality};
use chrono::{NaiveDate, NaiveTime};
use eframe::egui::{self, RichText, Ui};

pub struct State {
    loaded_for: Option<Option<i64>>,
    a: Activity,
    date: NaiveDate,
    time: String,
    duration: String,
    distance: String,
    calories: String,
    avg_hr: String,
    max_hr: String,
    incline: String,
    error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        let now = chrono::Local::now();
        State {
            loaded_for: None,
            a: Activity {
                modality: Modality::Strength,
                source: "manual".into(),
                ..Default::default()
            },
            date: now.date_naive(),
            time: now.format("%H:%M").to_string(),
            duration: "60".into(),
            distance: String::new(),
            calories: String::new(),
            avg_hr: String::new(),
            max_hr: String::new(),
            incline: String::new(),
            error: None,
        }
    }
}

fn opt(v: Option<f64>, d: usize) -> String {
    v.map(|v| format::num_trim(v, d)).unwrap_or_default()
}

pub fn show(app: &mut App, ui: &mut Ui, id: Option<i64>) {
    if app.pages.manual.loaded_for != Some(id) {
        let mut s = State::default();
        if let Some(a) = id.and_then(|i| app.data.get(i)).cloned() {
            let ls = a.local_start();
            s.date = ls.date();
            s.time = ls.format("%H:%M").to_string();
            s.duration = a.elapsed_s.map(format::duration).unwrap_or_default();
            s.distance = opt(a.distance_m.map(|d| d / 1000.0), 3);
            s.calories = opt(a.calories, 0);
            s.avg_hr = opt(a.avg_hr, 0);
            s.max_hr = opt(a.max_hr, 0);
            s.incline = opt(a.incline_pct, 1);
            s.a = a;
        }
        s.loaded_for = Some(id);
        app.pages.manual = s;
    }
    let editing = id.is_some();
    ui.horizontal(|ui| {
        if ui.button(tr!("← Voltar", "← Back")).clicked() {
            app.back();
        }
        widgets::page_title(
            ui,
            if editing {
                tr!("Editar atividade manual", "Edit manual activity")
            } else {
                tr!("Nova atividade manual", "New manual activity")
            },
        );
    });
    let s = &mut app.pages.manual;
    let mut save = false;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        egui::Grid::new("manual")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(tr!("Modalidade", "Type"));
                ui.horizontal(|ui| {
                    for m in Modality::ALL {
                        ui.selectable_value(&mut s.a.modality, m, m.label());
                    }
                });
                ui.end_row();
                ui.label(tr!("Título (opcional)", "Title (optional)"));
                let mut name = s.a.name.clone().unwrap_or_default();
                if ui
                    .add(egui::TextEdit::singleline(&mut name).desired_width(300.0))
                    .changed()
                {
                    s.a.name = Some(name).filter(|n| !n.trim().is_empty());
                }
                ui.end_row();
                ui.label(tr!("Data e horário", "Date and time"));
                ui.horizontal(|ui| {
                    widgets::date_picker(ui, &mut s.date, "manual-date");
                    ui.add(
                        egui::TextEdit::singleline(&mut s.time)
                            .desired_width(60.0)
                            .hint_text("07:00"),
                    );
                });
                ui.end_row();
                ui.label(tr!("Duração", "Duration"));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut s.duration).desired_width(80.0));
                    ui.label(weak(ui, tr!("minutos ou h:mm:ss", "minutes or h:mm:ss")));
                });
                ui.end_row();
                if s.a.modality != Modality::Strength {
                    ui.label(tr!("Distância (km)", "Distance (km)"));
                    ui.add(egui::TextEdit::singleline(&mut s.distance).desired_width(80.0));
                    ui.end_row();
                }
                if s.a.modality == Modality::Treadmill {
                    ui.label(tr!("Inclinação (%)", "Incline (%)"));
                    ui.add(egui::TextEdit::singleline(&mut s.incline).desired_width(80.0));
                    ui.end_row();
                }
                ui.label(tr!("Calorias", "Calories"));
                ui.add(egui::TextEdit::singleline(&mut s.calories).desired_width(80.0));
                ui.end_row();
                ui.label(tr!("FC média / máxima", "Avg / max HR"));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut s.avg_hr)
                            .desired_width(60.0)
                            .hint_text(tr!("méd", "avg")),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut s.max_hr)
                            .desired_width(60.0)
                            .hint_text(tr!("máx", "max")),
                    );
                    ui.label("bpm");
                });
                ui.end_row();
                ui.label(tr!("RPE (1–10)", "RPE (1–10)"));
                ui.horizontal(|ui| {
                    if ui.selectable_label(s.a.rpe.is_none(), "—").clicked() {
                        s.a.rpe = None;
                    }
                    for i in 1..=10 {
                        if ui.selectable_label(s.a.rpe == Some(i), i.to_string()).clicked() {
                            s.a.rpe = Some(i);
                        }
                    }
                });
                ui.end_row();
                ui.label(tr!("Observações", "Notes"));
                let mut notes = s.a.notes.clone().unwrap_or_default();
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut notes)
                            .desired_rows(3)
                            .desired_width(380.0),
                    )
                    .changed()
                {
                    s.a.notes = Some(notes).filter(|n| !n.trim().is_empty());
                }
                ui.end_row();
            });
        ui.label(
            weak(
                ui,
                tr!(
                    "Sem FC, a carga é estimada por RPE × minutos (ou esforço moderado, se não houver RPE).",
                    "Without HR, load is estimated as RPE × minutes (or a moderate effort when there is no RPE)."
                ),
            )
            .small(),
        );
        if s.a.modality == Modality::Strength {
            ui.label(weak(ui, tr!(
                "Os exercícios (séries × repetições × peso) são adicionados depois de salvar, na tela da atividade.",
                "Exercises (sets × reps × weight) are added after saving, on the activity screen."
            )));
        }
        if let Some(e) = &s.error {
            ui.label(RichText::new(e).color(widgets::BAD));
        }
        ui.add_space(8.0);
        if ui.button(RichText::new(tr!("💾 Salvar", "💾 Save")).strong()).clicked() {
            save = true;
        }
    });
    if save {
        let s = &mut app.pages.manual;
        let Some(dur) = format::parse_duration(&s.duration).filter(|d| *d > 0.0) else {
            s.error = Some(tr!("Informe a duração.", "Enter the duration.").into());
            return;
        };
        let time = NaiveTime::parse_from_str(s.time.trim(), "%H:%M")
            .unwrap_or_else(|_| NaiveTime::from_hms_opt(12, 0, 0).unwrap());
        let local = s.date.and_time(time);
        let offset = crate::db::local_offset_at(local.and_utc().timestamp());
        let mut a = s.a.clone();
        a.start_time = local.and_utc().timestamp() - offset;
        a.tz_offset = offset;
        a.elapsed_s = Some(dur);
        a.distance_m = format::parse_num(&s.distance).filter(|v| *v > 0.0).map(|v| v * 1000.0);
        a.calories = format::parse_num(&s.calories);
        a.avg_hr = format::parse_num(&s.avg_hr).filter(|v| (30.0..230.0).contains(v));
        a.max_hr = format::parse_num(&s.max_hr).filter(|v| (30.0..240.0).contains(v));
        a.incline_pct = format::parse_num(&s.incline);
        match app.db.save_manual_activity(&a) {
            Ok(new_id) => {
                if let Ok(Some(saved)) = app.db.activity(new_id) {
                    let st = app.settings.clone();
                    let obs = app.db.observed_max_hr();
                    let _ = metrics::update_activity(&app.db, &saved, &st, obs);
                }
                app.reload();
                app.toast(tr!("Atividade salva.", "Activity saved."));
                app.pages.manual.loaded_for = None;
                app.page = Page::Activity(new_id);
            }
            Err(e) => app.toast_error(e.to_string()),
        }
    }
}
