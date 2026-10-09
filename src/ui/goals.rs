//! Goals (item 17) with visual progress, and long-term objectives made of
//! sub-goals (item 18).

use super::App;
use super::widgets::{self, weak};
use crate::analysis::goals::{self, Progress};
use crate::export::best_efforts_map;
use crate::format;
use crate::model::Modality;
use crate::planning::{Goal, GoalKind, GoalParams, GoalPeriod, Objective};
use eframe::egui::{self, RichText, Ui};

#[derive(Default)]
pub struct State {
    edit: Option<GoalEdit>,
    obj_edit: Option<Objective>,
    show_archived: bool,
}

struct GoalEdit {
    g: Goal,
    distance: String,
    time: String,
    pace: String,
    hr: String,
    count: String,
    hours: String,
    use_dates: bool,
    start: chrono::NaiveDate,
    end: chrono::NaiveDate,
}

impl GoalEdit {
    fn new(g: Goal) -> GoalEdit {
        let p = &g.params;
        let today = chrono::Local::now().date_naive();
        GoalEdit {
            distance: p
                .distance_m
                .map(|d| format::num_trim(d / 1000.0, 2))
                .unwrap_or_default(),
            time: p.time_s.map(format::duration).unwrap_or_default(),
            pace: p.pace.map(format::pace).unwrap_or_default(),
            hr: p.hr.map(|h| format!("{}", h.round())).unwrap_or_default(),
            count: p.count.map(|c| format::num_trim(c, 1)).unwrap_or_default(),
            hours: p.hours.map(|h| format::num_trim(h, 1)).unwrap_or_default(),
            use_dates: g.start_date.is_some() || g.end_date.is_some(),
            start: g.start_date.unwrap_or(today),
            end: g.end_date.unwrap_or(today + chrono::Duration::days(90)),
            g,
        }
    }
}

fn modality_label(code: &Option<String>) -> &'static str {
    match code {
        None => tr!("Corrida (rua + esteira)", "Running (road + treadmill)"),
        Some(c) => Modality::from_code(c).label(),
    }
}

fn default_title(e: &GoalEdit) -> String {
    let p = &e.g.params;
    let km = format::parse_num(&e.distance).unwrap_or(0.0);
    match e.g.kind {
        GoalKind::Distance => trf!(
            "Correr {} km {}",
            "Run {} km {}",
            format::num_trim(km, 1),
            p.period.label()
        ),
        GoalKind::Frequency => trf!(
            "{} {} vezes por semana",
            "{} {} times a week",
            modality_label(&p.modality),
            e.count
        ),
        GoalKind::Performance => trf!("{} km abaixo de {}", "{} km under {}", format::num_trim(km, 1), e.time),
        GoalKind::HeartRate => trf!(
            "{} km a {}/km com FC média abaixo de {} bpm",
            "{} km at {}/km with avg HR below {} bpm",
            format::num_trim(km, 1),
            e.pace,
            e.hr
        ),
        GoalKind::MaxDistance => trf!("Chegar a {} km", "Reach {} km", format::num_trim(km, 1)),
        GoalKind::Hours => trf!(
            "{}: {} horas {}",
            "{}: {} hours {}",
            modality_label(&p.modality),
            e.hours,
            p.period.label()
        ),
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let today = chrono::Local::now().date_naive();
    ui.horizontal(|ui| {
        widgets::page_title(ui, tr!("Metas", "Goals"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(tr!("➕ Objetivo de longo prazo", "➕ Long-term objective"))
                .clicked()
            {
                app.pages.goals.obj_edit = Some(Objective {
                    title: tr!("Correr 10 km confortavelmente", "Run 10 km comfortably").into(),
                    ..Default::default()
                });
            }
            if ui.button(tr!("➕ Nova meta", "➕ New goal")).clicked() {
                app.pages.goals.edit = Some(GoalEdit::new(Goal {
                    kind: GoalKind::Distance,
                    params: GoalParams {
                        distance_m: Some(100_000.0),
                        period: GoalPeriod::Month,
                        ..Default::default()
                    },
                    ..Default::default()
                }));
            }
            ui.checkbox(
                &mut app.pages.goals.show_archived,
                tr!("Mostrar arquivadas", "Show archived"),
            );
        });
    });
    let all = app.db.goals().unwrap_or_default();
    let objectives = app.db.objectives().unwrap_or_default();
    let be = best_efforts_map(&app.db);
    let acts = app.data.activities.clone();
    let acts = &acts;

    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if app.pages.goals.edit.is_some() {
            goal_form(app, ui, &objectives);
            return;
        }
        if app.pages.goals.obj_edit.is_some() {
            objective_form(app, ui);
            return;
        }
        let show_arch = app.pages.goals.show_archived;
        let free: Vec<&Goal> = all.iter().filter(|g| g.objective_id.is_none() && (show_arch || !g.archived)).collect();
        if free.is_empty() && objectives.is_empty() {
            widgets::empty_state(ui, tr!(
                "Nenhuma meta. Exemplos: correr 100 km no mês, correr 3 vezes por semana, 5 km abaixo de 28 minutos, jogar 4 horas de tênis no mês.",
                "No goals. Examples: run 100 km a month, run 3 times a week, 5 km under 28 minutes, play 4 hours of tennis a month."
            ));
        }
        let mut action: Option<(i64, &'static str)> = None;
        for g in free {
            let p = goals::progress(g, acts, &be, today);
            goal_card(ui, g, &p, &mut action);
        }
        for o in &objectives {
            ui.add_space(10.0);
            let subs: Vec<&Goal> = all.iter().filter(|g| g.objective_id == Some(o.id)).collect();
            let progress: Vec<Progress> = subs.iter().map(|g| goals::progress(g, acts, &be, today)).collect();
            let done = progress.iter().filter(|p| p.done).count();
            widgets::card_frame(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("🏁 {}", o.title)).size(18.0).strong());
                    if let Some(d) = o.target_date {
                        ui.label(weak(ui, trf!("até {}", "by {}", format::date(d))));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(RichText::new("🗑").color(widgets::BAD)).on_hover_text(tr!("Excluir objetivo e submetas", "Delete objective and sub-goals")).clicked() {
                            action = Some((o.id, "del-obj"));
                        }
                        if ui.small_button(tr!("✏", "✏")).clicked() {
                            action = Some((o.id, "edit-obj"));
                        }
                        if ui.small_button(tr!("➕ submeta", "➕ sub-goal")).clicked() {
                            action = Some((o.id, "add-sub"));
                        }
                        if subs.is_empty() && ui.small_button(tr!("Gerar 5→10 km", "Generate 5→10 km")).on_hover_text(tr!(
                            "Cria as submetas de distância máxima 5, 6, 7, 8, 9 e 10 km",
                            "Creates longest-distance sub-goals of 5, 6, 7, 8, 9 and 10 km"
                        )).clicked() {
                            action = Some((o.id, "gen"));
                        }
                    });
                });
                if !o.description.is_empty() {
                    ui.label(weak(ui, &o.description));
                }
                if !subs.is_empty() {
                    widgets::progress(ui, done as f32 / subs.len() as f32, &trf!("{done} de {} submetas concluídas", "{done} of {} sub-goals done", subs.len()), done == subs.len());
                }
                for (g, p) in subs.iter().zip(&progress) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(if p.done { "✅" } else { "⬜" }).size(16.0));
                        ui.label(RichText::new(&g.title).strong());
                        ui.label(weak(ui, format!("{} {}", p.text, if p.detail.is_empty() { String::new() } else { format!("· {}", p.detail) })));
                        if ui.small_button("✏").clicked() {
                            action = Some((g.id, "edit"));
                        }
                        if ui.small_button("✖").clicked() {
                            action = Some((g.id, "del"));
                        }
                    });
                }
            });
        }
        if let Some((id, what)) = action {
            match what {
                "edit" => {
                    if let Some(g) = all.iter().find(|g| g.id == id) {
                        app.pages.goals.edit = Some(GoalEdit::new(g.clone()));
                    }
                }
                "del" => {
                    let _ = app.db.delete_goal(id);
                }
                "archive" => {
                    if let Some(mut g) = all.iter().find(|g| g.id == id).cloned() {
                        g.archived = !g.archived;
                        let _ = app.db.save_goal(&g);
                    }
                }
                "edit-obj" => app.pages.goals.obj_edit = objectives.iter().find(|o| o.id == id).cloned(),
                "del-obj" => {
                    let _ = app.db.delete_objective(id);
                }
                "add-sub" => {
                    app.pages.goals.edit = Some(GoalEdit::new(Goal {
                        kind: GoalKind::MaxDistance,
                        objective_id: Some(id),
                        params: GoalParams { distance_m: Some(5000.0), period: GoalPeriod::Range, ..Default::default() },
                        ..Default::default()
                    }));
                }
                "gen" => {
                    for (i, km) in (5..=10).enumerate() {
                        let g = Goal {
                            kind: GoalKind::MaxDistance,
                            title: format!("{km} km"),
                            objective_id: Some(id),
                            position: i as i64,
                            params: GoalParams { distance_m: Some(km as f64 * 1000.0), period: GoalPeriod::Range, ..Default::default() },
                            ..Default::default()
                        };
                        let _ = app.db.save_goal(&g);
                    }
                }
                _ => {}
            }
        }
    });
}

fn goal_card(ui: &mut Ui, g: &Goal, p: &Progress, action: &mut Option<(i64, &'static str)>) {
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(&g.title).size(16.0).strong());
            widgets::chip(ui, g.kind.label(), widgets::ACCENT);
            if p.done {
                widgets::chip(ui, tr!("✔ atingida", "✔ reached"), widgets::GOOD);
            }
            if g.archived {
                widgets::chip(ui, tr!("arquivada", "archived"), egui::Color32::GRAY);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("✖").on_hover_text(tr!("Excluir", "Delete")).clicked() {
                    *action = Some((g.id, "del"));
                }
                if ui
                    .small_button("🗄")
                    .on_hover_text(tr!("Arquivar / desarquivar", "Archive / unarchive"))
                    .clicked()
                {
                    *action = Some((g.id, "archive"));
                }
                if ui.small_button("✏").clicked() {
                    *action = Some((g.id, "edit"));
                }
            });
        });
        let pct = (p.fraction.min(1.0) * 100.0).round();
        widgets::progress(ui, p.fraction, &format!("{} — {pct}%", p.text), p.done);
        if !p.detail.is_empty() {
            ui.label(weak(ui, &p.detail).small());
        }
    });
    ui.add_space(4.0);
}

fn goal_form(app: &mut App, ui: &mut Ui, objectives: &[Objective]) {
    let Some(e) = app.pages.goals.edit.as_mut() else { return };
    let mut save = false;
    let mut cancel = false;
    ui.label(
        RichText::new(if e.g.id == 0 {
            tr!("Nova meta", "New goal")
        } else {
            tr!("Editar meta", "Edit goal")
        })
        .size(18.0)
        .strong(),
    );
    egui::Grid::new("goal-form")
        .num_columns(2)
        .spacing([10.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr!("Tipo", "Kind"));
            ui.horizontal_wrapped(|ui| {
                for k in GoalKind::ALL {
                    ui.selectable_value(&mut e.g.kind, k, k.label());
                }
            });
            ui.end_row();
            let k = e.g.kind;
            if matches!(k, GoalKind::Frequency | GoalKind::Hours | GoalKind::Distance) {
                ui.label(tr!("Modalidade", "Type"));
                egui::ComboBox::from_id_salt("goal-mod")
                    .selected_text(modality_label(&e.g.params.modality))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut e.g.params.modality, None, modality_label(&None));
                        for m in Modality::ALL {
                            ui.selectable_value(&mut e.g.params.modality, Some(m.code().to_string()), m.label());
                        }
                    });
                ui.end_row();
            }
            if matches!(k, GoalKind::Distance | GoalKind::Hours) {
                ui.label(tr!("Período", "Period"));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut e.g.params.period, GoalPeriod::Week, tr!("semana", "week"));
                    ui.selectable_value(&mut e.g.params.period, GoalPeriod::Month, tr!("mês", "month"));
                    ui.selectable_value(
                        &mut e.g.params.period,
                        GoalPeriod::Range,
                        tr!("datas da meta", "goal dates"),
                    );
                });
                ui.end_row();
            }
            if matches!(
                k,
                GoalKind::Distance | GoalKind::Performance | GoalKind::HeartRate | GoalKind::MaxDistance
            ) {
                ui.label(tr!("Distância (km)", "Distance (km)"));
                ui.add(egui::TextEdit::singleline(&mut e.distance).desired_width(70.0));
                ui.end_row();
            }
            if k == GoalKind::Performance {
                ui.label(tr!("Tempo abaixo de", "Time under"));
                ui.add(
                    egui::TextEdit::singleline(&mut e.time)
                        .desired_width(70.0)
                        .hint_text("28:00"),
                );
                ui.end_row();
            }
            if k == GoalKind::HeartRate {
                ui.label(tr!("No pace (ou mais rápido)", "At pace (or faster)"));
                ui.add(
                    egui::TextEdit::singleline(&mut e.pace)
                        .desired_width(70.0)
                        .hint_text("6:00"),
                );
                ui.end_row();
                ui.label(tr!("FC média abaixo de", "Avg HR below"));
                ui.add(
                    egui::TextEdit::singleline(&mut e.hr)
                        .desired_width(70.0)
                        .hint_text("165"),
                );
                ui.end_row();
            }
            if k == GoalKind::Frequency {
                ui.label(tr!("Vezes por semana", "Times per week"));
                ui.add(
                    egui::TextEdit::singleline(&mut e.count)
                        .desired_width(70.0)
                        .hint_text("3"),
                );
                ui.end_row();
            }
            if k == GoalKind::Hours {
                ui.label(tr!("Horas", "Hours"));
                ui.add(
                    egui::TextEdit::singleline(&mut e.hours)
                        .desired_width(70.0)
                        .hint_text("4"),
                );
                ui.end_row();
            }
            ui.label(tr!("Datas (opcional)", "Dates (optional)"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut e.use_dates, "");
                if e.use_dates {
                    widgets::date_picker(ui, &mut e.start, "goal-start");
                    ui.label("→");
                    widgets::date_picker(ui, &mut e.end, "goal-end");
                }
            });
            ui.end_row();
            ui.label(tr!("Objetivo de longo prazo", "Long-term objective"));
            egui::ComboBox::from_id_salt("goal-obj")
                .selected_text(
                    e.g.objective_id
                        .and_then(|id| objectives.iter().find(|o| o.id == id))
                        .map(|o| o.title.as_str())
                        .unwrap_or("—"),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut e.g.objective_id, None, "—");
                    for o in objectives {
                        ui.selectable_value(&mut e.g.objective_id, Some(o.id), &o.title);
                    }
                });
            ui.end_row();
            ui.label(tr!("Título", "Title"));
            let hint = default_title(e);
            ui.add(
                egui::TextEdit::singleline(&mut e.g.title)
                    .desired_width(360.0)
                    .hint_text(hint),
            );
            ui.end_row();
        });
    ui.horizontal(|ui| {
        save = ui.button(RichText::new(tr!("💾 Salvar", "💾 Save")).strong()).clicked();
        cancel = ui.button(tr!("Cancelar", "Cancel")).clicked();
    });
    if save {
        let p = &mut e.g.params;
        p.distance_m = format::parse_num(&e.distance).filter(|v| *v > 0.0).map(|v| v * 1000.0);
        p.time_s = format::parse_duration(&e.time).filter(|v| *v > 0.0);
        p.pace = format::parse_pace(&e.pace);
        p.hr = format::parse_num(&e.hr);
        p.count = format::parse_num(&e.count);
        p.hours = format::parse_num(&e.hours);
        if e.g.title.trim().is_empty() {
            e.g.title = default_title(e);
        }
        e.g.start_date = e.use_dates.then_some(e.start);
        e.g.end_date = e.use_dates.then_some(e.end);
        let g = e.g.clone();
        match app.db.save_goal(&g) {
            Ok(_) => app.pages.goals.edit = None,
            Err(err) => app.toast_error(err.to_string()),
        }
    } else if cancel {
        app.pages.goals.edit = None;
    }
}

fn objective_form(app: &mut App, ui: &mut Ui) {
    let Some(o) = app.pages.goals.obj_edit.as_mut() else {
        return;
    };
    let mut save = false;
    let mut cancel = false;
    ui.label(
        RichText::new(tr!("Objetivo de longo prazo", "Long-term objective"))
            .size(18.0)
            .strong(),
    );
    egui::Grid::new("obj-form")
        .num_columns(2)
        .spacing([10.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr!("Objetivo", "Objective"));
            ui.add(egui::TextEdit::singleline(&mut o.title).desired_width(360.0));
            ui.end_row();
            ui.label(tr!("Descrição", "Description"));
            ui.add(
                egui::TextEdit::multiline(&mut o.description)
                    .desired_rows(2)
                    .desired_width(360.0),
            );
            ui.end_row();
            ui.label(tr!("Data alvo", "Target date"));
            ui.horizontal(|ui| {
                let mut on = o.target_date.is_some();
                ui.checkbox(&mut on, "");
                if on {
                    let mut d = o
                        .target_date
                        .unwrap_or_else(|| chrono::Local::now().date_naive() + chrono::Duration::days(180));
                    widgets::date_picker(ui, &mut d, "obj-date");
                    o.target_date = Some(d);
                } else {
                    o.target_date = None;
                }
            });
            ui.end_row();
        });
    ui.label(weak(
        ui,
        tr!(
            "Depois de salvar, adicione as submetas (ex.: 5, 6, 7, 8, 9 e 10 km) no cartão do objetivo.",
            "After saving, add the sub-goals (e.g. 5, 6, 7, 8, 9 and 10 km) on the objective card."
        ),
    ));
    ui.horizontal(|ui| {
        save = ui.button(RichText::new(tr!("💾 Salvar", "💾 Save")).strong()).clicked();
        cancel = ui.button(tr!("Cancelar", "Cancel")).clicked();
    });
    if save {
        let o = o.clone();
        match app.db.save_objective(&o) {
            Ok(_) => app.pages.goals.obj_edit = None,
            Err(err) => app.toast_error(err.to_string()),
        }
    } else if cancel {
        app.pages.goals.obj_edit = None;
    }
}
