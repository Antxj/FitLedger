//! Training plans (item 13): name, goal, objectives, dates, sessions per
//! week, modalities; a weekly template that fills the calendar, and the
//! list of the plan's workouts (planned vs done).

use super::App;
use super::calendar::{self, Editor};
use super::widgets::{self, weak};
use crate::analysis::load::week_start;
use crate::format;
use crate::model::Modality;
use crate::planning::{Plan, PlannedWorkout, RunType};
use chrono::{Datelike, Duration, NaiveDate};
use eframe::egui::{self, RichText, Ui};

/// Option of the weekly template for one weekday.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum DayKind {
    #[default]
    Rest,
    Run(RunType),
    Strength,
    Tennis,
}

impl DayKind {
    fn label(self) -> &'static str {
        match self {
            DayKind::Rest => tr!("Descanso", "Rest"),
            DayKind::Run(r) => r.label(),
            DayKind::Strength => Modality::Strength.label(),
            DayKind::Tennis => Modality::Tennis.label(),
        }
    }

    fn options() -> Vec<DayKind> {
        let mut v = vec![DayKind::Rest, DayKind::Strength, DayKind::Tennis];
        v.extend(RunType::ALL.into_iter().map(DayKind::Run));
        v
    }
}

pub struct State {
    selected: Option<i64>,
    edit: Option<PlanEdit>,
    template: [DayKind; 7],
    template_treadmill: bool,
    confirm_delete: bool,
}

impl Default for State {
    fn default() -> Self {
        State {
            selected: None,
            edit: None,
            // the example of the request: strength, easy run, rest, intervals, strength, tennis, long run
            template: [
                DayKind::Strength,
                DayKind::Run(RunType::Easy),
                DayKind::Rest,
                DayKind::Run(RunType::Intervals),
                DayKind::Strength,
                DayKind::Tennis,
                DayKind::Run(RunType::Long),
            ],
            template_treadmill: false,
            confirm_delete: false,
        }
    }
}

struct PlanEdit {
    p: Plan,
    start: NaiveDate,
    end: NaiveDate,
    sessions: String,
}

impl PlanEdit {
    fn new(p: Plan) -> PlanEdit {
        let today = chrono::Local::now().date_naive();
        PlanEdit {
            start: p.start_date.unwrap_or(week_start(today)),
            end: p.end_date.unwrap_or(week_start(today) + Duration::days(8 * 7 - 1)),
            sessions: p.sessions_per_week.map(|v| v.to_string()).unwrap_or_default(),
            p,
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let plans = app.db.plans().unwrap_or_default();
    ui.horizontal(|ui| {
        widgets::page_title(ui, tr!("Planos", "Plans"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr!("➕ Novo plano", "➕ New plan")).clicked() {
                app.pages.plans.edit = Some(PlanEdit::new(Plan {
                    name: tr!("Melhorar eficiência nos 5 km", "Improve 5 km efficiency").into(),
                    goal: tr!("Correr 5 km a 6:00/km com menor frequência cardíaca", "Run 5 km at 6:00/km with a lower heart rate").into(),
                    objectives: tr!(
                        "Correr 5 km a 6:00/km com menor frequência cardíaca\nAumentar corrida longa até 10 km\nManter tênis\nManter musculação",
                        "Run 5 km at 6:00/km with a lower heart rate\nIncrease the long run up to 10 km\nKeep tennis\nKeep strength training"
                    )
                    .into(),
                    sessions_per_week: Some(6),
                    modalities: vec![Modality::RoadRun, Modality::Treadmill, Modality::Tennis, Modality::Strength],
                    ..Default::default()
                }));
            }
        });
    });
    if app.pages.plans.selected.is_none() && app.pages.plans.edit.is_none() {
        app.pages.plans.selected = plans.first().map(|p| p.id);
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(230.0);
            if plans.is_empty() {
                ui.label(weak(ui, tr!("Nenhum plano ainda.", "No plans yet.")));
            }
            for p in &plans {
                let sel = app.pages.plans.selected == Some(p.id) && app.pages.plans.edit.is_none();
                let text = format!(
                    "{}\n{}",
                    p.name,
                    match (p.start_date, p.end_date) {
                        (Some(a), Some(b)) => format!("{} – {}", format::date_short(a), format::date(b)),
                        _ => String::new(),
                    }
                );
                if ui
                    .add_sized([220.0, 40.0], egui::Button::selectable(sel, text))
                    .clicked()
                {
                    app.pages.plans.selected = Some(p.id);
                    app.pages.plans.edit = None;
                }
            }
        });
        ui.separator();
        ui.vertical(|ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if app.pages.plans.edit.is_some() {
                    edit_form(app, ui);
                } else if let Some(p) = plans.iter().find(|p| Some(p.id) == app.pages.plans.selected).cloned() {
                    plan_view(app, ui, &p);
                } else {
                    widgets::empty_state(
                        ui,
                        tr!(
                            "Crie um plano para organizar seus treinos.",
                            "Create a plan to organize your workouts."
                        ),
                    );
                }
            });
        });
    });
    calendar::editor_window_public(app, ui.ctx());
}

fn edit_form(app: &mut App, ui: &mut Ui) {
    let Some(e) = app.pages.plans.edit.as_mut() else { return };
    let mut save = false;
    let mut cancel = false;
    egui::Grid::new("plan-edit")
        .num_columns(2)
        .spacing([10.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr!("Nome", "Name"));
            ui.add(egui::TextEdit::singleline(&mut e.p.name).desired_width(360.0));
            ui.end_row();
            ui.label(tr!("Objetivo", "Goal"));
            ui.add(egui::TextEdit::singleline(&mut e.p.goal).desired_width(360.0));
            ui.end_row();
            ui.label(tr!("Objetivos (um por linha)", "Objectives (one per line)"));
            ui.add(
                egui::TextEdit::multiline(&mut e.p.objectives)
                    .desired_rows(4)
                    .desired_width(360.0),
            );
            ui.end_row();
            ui.label(tr!("Período", "Period"));
            ui.horizontal(|ui| {
                widgets::date_picker(ui, &mut e.start, "plan-start");
                ui.label("→");
                widgets::date_picker(ui, &mut e.end, "plan-end");
            });
            ui.end_row();
            ui.label(tr!("Treinos por semana", "Workouts per week"));
            ui.add(egui::TextEdit::singleline(&mut e.sessions).desired_width(50.0));
            ui.end_row();
            ui.label(tr!("Modalidades", "Types"));
            ui.horizontal_wrapped(|ui| {
                for m in Modality::ALL {
                    let mut on = e.p.modalities.contains(&m);
                    if ui.checkbox(&mut on, m.label()).changed() {
                        if on {
                            e.p.modalities.push(m);
                        } else {
                            e.p.modalities.retain(|x| *x != m);
                        }
                    }
                }
            });
            ui.end_row();
        });
    ui.horizontal(|ui| {
        save = ui.button(RichText::new(tr!("💾 Salvar", "💾 Save")).strong()).clicked();
        cancel = ui.button(tr!("Cancelar", "Cancel")).clicked();
    });
    if save {
        e.p.start_date = Some(e.start);
        e.p.end_date = Some(e.end.max(e.start));
        e.p.sessions_per_week = e.sessions.trim().parse().ok();
        let p = e.p.clone();
        match app.db.save_plan(&p) {
            Ok(id) => {
                app.pages.plans.selected = Some(id);
                app.pages.plans.edit = None;
            }
            Err(err) => app.toast_error(err.to_string()),
        }
    } else if cancel {
        app.pages.plans.edit = None;
    }
}

fn plan_view(app: &mut App, ui: &mut Ui, p: &Plan) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(&p.name).size(20.0).strong());
        if ui.small_button(tr!("✏ Editar", "✏ Edit")).clicked() {
            app.pages.plans.edit = Some(PlanEdit::new(p.clone()));
        }
        if ui
            .small_button(RichText::new(tr!("🗑 Excluir", "🗑 Delete")).color(widgets::BAD))
            .clicked()
        {
            app.pages.plans.confirm_delete = true;
        }
    });
    if !p.goal.is_empty() {
        ui.label(RichText::new(&p.goal).italics());
    }
    let mut info = Vec::new();
    if let (Some(a), Some(b)) = (p.start_date, p.end_date) {
        let weeks = ((b - a).num_days() + 1) as f64 / 7.0;
        info.push(format!(
            "{} – {} ({} {})",
            format::date(a),
            format::date(b),
            format::num(weeks, 0),
            tr!("semanas", "weeks")
        ));
    }
    if let Some(n) = p.sessions_per_week {
        info.push(trf!("{n} treinos por semana", "{n} workouts per week"));
    }
    ui.label(weak(ui, info.join(" · ")));
    ui.horizontal_wrapped(|ui| {
        for m in &p.modalities {
            widgets::modality_chip(ui, *m);
        }
    });
    if !p.objectives.trim().is_empty() {
        widgets::section(ui, tr!("Objetivos", "Objectives"));
        for line in p.objectives.lines().filter(|l| !l.trim().is_empty()) {
            ui.label(format!("• {}", line.trim()));
        }
    }

    let workouts = app.db.planned_workouts_of_plan(p.id).unwrap_or_default();
    let today = chrono::Local::now().date_naive();
    let done = workouts.iter().filter(|w| w.activity_id.is_some()).count();
    let past = workouts.iter().filter(|w| w.date <= today).count();
    widgets::section(ui, tr!("Execução", "Adherence"));
    if past > 0 {
        widgets::progress(
            ui,
            done as f32 / past as f32,
            &trf!(
                "{done} de {past} treinos até hoje realizados",
                "{done} of {past} workouts so far done"
            ),
            done == past,
        );
    }

    widgets::section(ui, tr!("Modelo semanal", "Weekly template"));
    ui.label(weak(ui, tr!(
        "Escolha o treino de cada dia e gere os treinos planejados do período inteiro no calendário. Depois ajuste cada um (pace, distância, intervalos) no calendário.",
        "Choose each day's workout and generate the planned workouts for the whole period in the calendar. Then adjust each one (pace, distance, intervals) in the calendar."
    )));
    let s = &mut app.pages.plans;
    ui.horizontal_wrapped(|ui| {
        let monday = week_start(today);
        for i in 0..7 {
            ui.vertical(|ui| {
                ui.label(RichText::new(format::weekday_short(monday + Duration::days(i as i64))).strong());
                egui::ComboBox::from_id_salt(format!("tpl{i}"))
                    .width(96.0)
                    .selected_text(s.template[i].label())
                    .show_ui(ui, |ui| {
                        for k in DayKind::options() {
                            ui.selectable_value(&mut s.template[i], k, k.label());
                        }
                    });
            });
        }
    });
    ui.checkbox(
        &mut s.template_treadmill,
        tr!("Corridas na esteira", "Runs on the treadmill"),
    );
    let template = s.template;
    let treadmill = s.template_treadmill;
    if ui
        .button(tr!(
            "📅 Gerar treinos no calendário",
            "📅 Generate workouts in the calendar"
        ))
        .clicked()
    {
        match (p.start_date, p.end_date) {
            (Some(a), Some(b)) => {
                let mut n = 0;
                let mut d = a.max(today);
                while d <= b {
                    let kind = template[d.weekday().num_days_from_monday() as usize];
                    let exists = workouts.iter().any(|w| w.date == d);
                    if kind != DayKind::Rest && !exists {
                        let mut w = PlannedWorkout::new(d);
                        w.plan_id = Some(p.id);
                        match kind {
                            DayKind::Run(r) => {
                                w.modality = if treadmill {
                                    Modality::Treadmill
                                } else {
                                    Modality::RoadRun
                                };
                                w.run_type = Some(r);
                            }
                            DayKind::Strength => {
                                w.modality = Modality::Strength;
                                w.run_type = None;
                            }
                            DayKind::Tennis => {
                                w.modality = Modality::Tennis;
                                w.run_type = None;
                            }
                            DayKind::Rest => {}
                        }
                        if app.db.save_planned_workout(&w).is_ok() {
                            n += 1;
                        }
                    }
                    d += Duration::days(1);
                }
                app.toast(trf!(
                    "{n} treinos criados no calendário.",
                    "{n} workouts created in the calendar."
                ));
            }
            _ => app.toast_error(tr!("Defina as datas do plano.", "Set the plan dates.")),
        }
    }

    widgets::section(ui, tr!("Treinos do plano", "Plan workouts"));
    if workouts.is_empty() {
        ui.label(weak(ui, tr!("Nenhum treino ainda.", "No workouts yet.")));
    }
    let mut open = None;
    egui::Grid::new("plan-workouts")
        .striped(true)
        .num_columns(4)
        .show(ui, |ui| {
            for w in workouts
                .iter()
                .filter(|w| w.date >= today - Duration::days(14))
                .take(60)
            {
                ui.label(format!(
                    "{} {}",
                    format::weekday_short(w.date),
                    format::date_short(w.date)
                ));
                if ui
                    .link(RichText::new(w.display_title()).color(widgets::modality_color(w.modality)))
                    .clicked()
                {
                    open = Some(w.clone());
                }
                ui.label(weak(ui, calendar::summary(w)));
                if w.activity_id.is_some() {
                    widgets::chip(ui, tr!("✔ feito", "✔ done"), widgets::GOOD);
                } else if w.date < today {
                    widgets::chip(ui, tr!("não feito", "missed"), widgets::WARN);
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        });
    if let Some(w) = open {
        app.pages.calendar.editor = Some(Editor::new(w));
    }
    let ctx = ui.ctx().clone();
    if widgets::confirm_window(
        &ctx,
        &mut app.pages.plans.confirm_delete,
        tr!("Excluir plano", "Delete plan"),
        tr!(
            "Excluir este plano? Os treinos planejados continuam no calendário, sem plano.",
            "Delete this plan? Its planned workouts stay in the calendar, without a plan."
        ),
    ) {
        let _ = app.db.delete_plan(p.id);
        app.pages.plans.selected = None;
    }
}
