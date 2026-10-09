//! Calendar (items 14, 15, 16, 29): month and week views, planned vs done,
//! day diary with accumulated load, planned workout editor with structured
//! intervals, and linking an activity to a planned workout.

use super::widgets::{self, weak};
use super::{App, Page};
use crate::analysis::load::{self, day_of, week_start};
use crate::format;
use crate::model::{Activity, Modality};
use crate::planning::{PlannedWorkout, RunType, Step, StepKind, steps_totals};
use chrono::{Datelike, Duration, NaiveDate};
use eframe::egui::{self, Color32, RichText, Stroke, Ui};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Month,
    Week,
}

pub struct State {
    view: View,
    cursor: NaiveDate,
    selected: Option<NaiveDate>,
    pub editor: Option<Editor>,
}

impl Default for State {
    fn default() -> Self {
        let today = chrono::Local::now().date_naive();
        State {
            view: View::Month,
            cursor: today,
            selected: Some(today),
            editor: None,
        }
    }
}

/// Planned workout being edited, with text buffers.
pub struct Editor {
    pub w: PlannedWorkout,
    duration: String,
    distance: String,
    pace: String,
    hr: String,
    incline: String,
    /// Buffers of the steps, by path ("0", "1.2"...).
    bufs: std::collections::HashMap<String, (String, String, String)>,
    pub open: bool,
}

impl Editor {
    pub fn new(w: PlannedWorkout) -> Editor {
        let opt = |v: Option<f64>, f: &dyn Fn(f64) -> String| v.map(f).unwrap_or_default();
        Editor {
            duration: opt(w.duration_s, &|v| format::duration(v)),
            distance: opt(w.distance_m, &|v| format::num_trim(v / 1000.0, 2)),
            pace: opt(w.target_pace, &|v| format::pace(v)),
            hr: opt(w.target_hr, &|v| format!("{}", v.round())),
            incline: opt(w.incline_pct, &|v| format::num_trim(v, 1)),
            bufs: Default::default(),
            open: true,
            w,
        }
    }
}

type CellItem = (String, Color32, bool, Option<i64>, Option<i64>);

fn month_grid_start(d: NaiveDate) -> NaiveDate {
    week_start(d.with_day(1).unwrap_or(d))
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let today = chrono::Local::now().date_naive();
    ui.horizontal(|ui| {
        widgets::page_title(ui, tr!("Calendário", "Calendar"));
        ui.add_space(20.0);
        let s = &mut app.pages.calendar;
        ui.selectable_value(&mut s.view, View::Month, tr!("Mês", "Month"));
        ui.selectable_value(&mut s.view, View::Week, tr!("Semana", "Week"));
        ui.separator();
        let step = |d: NaiveDate, fwd: bool, v: View| match v {
            View::Month => {
                let m = chrono::Months::new(1);
                let first = d.with_day(1).unwrap_or(d);
                if fwd {
                    first.checked_add_months(m)
                } else {
                    first.checked_sub_months(m)
                }
                .unwrap_or(d)
            }
            View::Week => d + Duration::days(if fwd { 7 } else { -7 }),
        };
        if ui.button("◀").clicked() {
            s.cursor = step(s.cursor, false, s.view);
        }
        if ui.button(tr!("Hoje", "Today")).clicked() {
            s.cursor = today;
            s.selected = Some(today);
        }
        if ui.button("▶").clicked() {
            s.cursor = step(s.cursor, true, s.view);
        }
        let title = match s.view {
            View::Month => format!("{} {}", format::month_name(s.cursor.month()), s.cursor.year()),
            View::Week => {
                let ws = week_start(s.cursor);
                format!("{} – {}", format::date_short(ws), format::date(ws + Duration::days(6)))
            }
        };
        ui.label(RichText::new(title).size(17.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr!("➕ Planejar treino", "➕ Plan a workout")).clicked() {
                let d = s.selected.unwrap_or(today);
                s.editor = Some(Editor::new(PlannedWorkout::new(d)));
            }
        });
    });
    ui.horizontal(|ui| {
        legend(ui);
    });
    let (from, days) = match app.pages.calendar.view {
        View::Month => (month_grid_start(app.pages.calendar.cursor), 42),
        View::Week => (week_start(app.pages.calendar.cursor), 7),
    };
    let to = from + Duration::days(days - 1);
    let planned = app.db.planned_workouts(from, to).unwrap_or_default();
    let acts: Vec<Activity> = app
        .data
        .activities
        .iter()
        .filter(|a| (from..=to).contains(&day_of(a)))
        .cloned()
        .collect();

    let avail = ui.available_size();
    let side = 330.0_f32.min(avail.x * 0.35);
    ui.horizontal_top(|ui| {
        let grid_w = avail.x - side - 12.0;
        ui.vertical(|ui| {
            ui.set_width(grid_w);
            grid(app, ui, from, days, &planned, &acts, grid_w, today);
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_width(side);
            egui::ScrollArea::vertical()
                .id_salt("day-panel")
                .show(ui, |ui| day_panel(app, ui, &planned));
        });
    });
    editor_window(app, ui.ctx());
}

fn legend(ui: &mut Ui) {
    let item = |ui: &mut Ui, filled: bool, text: &str| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 10.0), egui::Sense::hover());
        if filled {
            ui.painter().rect_filled(r, 3.0, widgets::ACCENT);
        } else {
            ui.painter()
                .rect_stroke(r, 3.0, Stroke::new(1.5, widgets::ACCENT), egui::StrokeKind::Inside);
        }
        ui.label(weak(ui, text).small());
    };
    item(ui, true, tr!("Realizado", "Done"));
    item(ui, false, tr!("Planejado", "Planned"));
    ui.label(
        weak(
            ui,
            tr!("✔ = planejado e feito (vinculado)", "✔ = planned and done (linked)"),
        )
        .small(),
    );
}

#[allow(clippy::too_many_arguments)]
fn grid(
    app: &mut App,
    ui: &mut Ui,
    from: NaiveDate,
    days: i64,
    planned: &[PlannedWorkout],
    acts: &[Activity],
    width: f32,
    today: NaiveDate,
) {
    let col_w = (width - 6.0 * 4.0) / 7.0;
    let cell_h = if days == 7 { 360.0 } else { 92.0 };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for i in 0..7 {
            let d = from + Duration::days(i);
            ui.add_sized(
                [col_w, 18.0],
                egui::Label::new(RichText::new(format::weekday_short(d)).strong()),
            );
        }
    });
    let month = app.pages.calendar.cursor.month();
    let is_month = days > 7;
    let linked: std::collections::HashSet<i64> = planned.iter().filter_map(|w| w.activity_id).collect();
    let mut click_day = None;
    let mut open_act = None;
    let mut open_plan = None;
    for week in 0..(days / 7) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for i in 0..7 {
                let d = from + Duration::days(week * 7 + i);
                let selected = app.pages.calendar.selected == Some(d);
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(col_w, cell_h), egui::Sense::click());
                let vis = ui.visuals();
                let mut fill = vis.faint_bg_color;
                if is_month && d.month() != month {
                    fill = vis.extreme_bg_color;
                }
                ui.painter().rect_filled(rect, 6.0, fill);
                let stroke = if selected {
                    Stroke::new(2.0, widgets::ACCENT)
                } else if d == today {
                    Stroke::new(1.5, widgets::WARN)
                } else {
                    vis.widgets.noninteractive.bg_stroke
                };
                ui.painter().rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);
                let text_color = if is_month && d.month() != month {
                    vis.weak_text_color()
                } else {
                    vis.text_color()
                };
                ui.painter().text(
                    rect.left_top() + egui::vec2(6.0, 4.0),
                    egui::Align2::LEFT_TOP,
                    d.day().to_string(),
                    egui::FontId::proportional(13.0),
                    text_color,
                );
                // items
                let mut y = rect.top() + 22.0;
                let line_h = 17.0;
                let max_y = rect.bottom() - 4.0;
                // (text, color, done, activity id, planned id)
                let mut items: Vec<CellItem> = Vec::new();
                for a in acts.iter().filter(|a| day_of(a) == d) {
                    let mut t = a.title();
                    if let Some(km) = a.distance().filter(|_| a.modality.is_run()) {
                        t = format!("{} {}", t, format::num(km / 1000.0, 1));
                    }
                    if linked.contains(&a.id) {
                        t = format!("✔ {t}");
                    }
                    items.push((t, widgets::modality_color(a.modality), true, Some(a.id), None));
                }
                for w in planned.iter().filter(|w| w.date == d && w.activity_id.is_none()) {
                    items.push((
                        w.display_title(),
                        widgets::modality_color(w.modality),
                        false,
                        None,
                        Some(w.id),
                    ));
                }
                let mut hidden = 0;
                for (text, color, done, aid, pid) in items {
                    if y + line_h > max_y {
                        hidden += 1;
                        continue;
                    }
                    let r = egui::Rect::from_min_size(
                        egui::pos2(rect.left() + 4.0, y),
                        egui::vec2(rect.width() - 8.0, line_h - 2.0),
                    );
                    if done {
                        ui.painter().rect_filled(r, 3.0, color.gamma_multiply(0.85));
                    } else {
                        ui.painter()
                            .rect_stroke(r, 3.0, Stroke::new(1.3, color), egui::StrokeKind::Inside);
                    }
                    let galley = ui.painter().layout(
                        text,
                        egui::FontId::proportional(11.5),
                        if done { Color32::WHITE } else { color },
                        r.width() - 6.0,
                    );
                    ui.painter()
                        .with_clip_rect(r)
                        .galley(r.left_top() + egui::vec2(3.0, 1.0), galley, Color32::WHITE);
                    let item_resp = ui.interact(r, ui.id().with((d, aid, pid)), egui::Sense::click());
                    if item_resp.clicked() {
                        if let Some(id) = aid {
                            open_act = Some(id);
                        }
                        if let Some(id) = pid {
                            open_plan = Some(id);
                        }
                        click_day = Some(d);
                    }
                    y += line_h;
                }
                if hidden > 0 {
                    ui.painter().text(
                        rect.right_top() + egui::vec2(-6.0, 4.0),
                        egui::Align2::RIGHT_TOP,
                        format!("+{hidden}"),
                        egui::FontId::proportional(11.0),
                        widgets::WARN,
                    );
                }
                if resp.clicked() {
                    click_day = Some(d);
                }
                if resp.double_clicked() {
                    app.pages.calendar.editor = Some(Editor::new(PlannedWorkout::new(d)));
                }
            }
        });
        ui.add_space(4.0);
    }
    if let Some(d) = click_day {
        app.pages.calendar.selected = Some(d);
    }
    if let Some(id) = open_act {
        app.go(Page::Activity(id));
    }
    if let Some(id) = open_plan
        && let Ok(Some(w)) = app.db.planned_workout(id)
    {
        app.pages.calendar.editor = Some(Editor::new(w));
    }
    ui.label(
        weak(
            ui,
            tr!(
                "Clique em um dia para ver o diário; duplo clique para planejar um treino.",
                "Click a day to see its diary; double-click to plan a workout."
            ),
        )
        .small(),
    );
}

fn day_panel(app: &mut App, ui: &mut Ui, planned_range: &[PlannedWorkout]) {
    let Some(d) = app.pages.calendar.selected else { return };
    ui.label(
        RichText::new(format!("{} {}", format::weekday_short(d), format::date(d)))
            .size(17.0)
            .strong(),
    );
    let mut acts: Vec<&Activity> = app.data.activities.iter().filter(|a| day_of(a) == d).collect();
    acts.sort_by_key(|a| a.start_time);
    let mut open = None;
    if acts.is_empty() {
        ui.label(weak(
            ui,
            tr!("Nenhuma atividade neste dia.", "No activity on this day."),
        ));
    }
    let mut day_load = 0.0;
    for a in &acts {
        let m = app.data.metrics(a.id);
        let l = load::activity_load(a, m);
        day_load += l;
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(a.local_start().format("%H:%M").to_string()).strong());
                if ui
                    .link(RichText::new(a.title()).color(widgets::modality_color(a.modality)))
                    .clicked()
                {
                    open = Some(a.id);
                }
            });
            let mut parts = vec![format::opt_duration(a.duration())];
            if a.distance().is_some() {
                parts.push(format::opt_km(a.distance()));
            }
            if a.avg_hr.is_some() {
                parts.push(format::bpm(a.avg_hr));
            }
            parts.push(trf!("carga {}", "load {}", l.round()));
            ui.label(weak(ui, parts.join(" · ")));
            if let Some(n) = &a.notes {
                ui.label(weak(ui, n).italics().small());
            }
        });
    }
    if acts.len() > 1 {
        ui.label(
            RichText::new(trf!(
                "Carga acumulada no dia: {}",
                "Accumulated load on the day: {}",
                day_load.round()
            ))
            .strong(),
        );
    }
    widgets::section(ui, tr!("Planejado", "Planned"));
    let planned: Vec<&PlannedWorkout> = planned_range.iter().filter(|w| w.date == d).collect();
    if planned.is_empty() {
        ui.label(weak(ui, tr!("Nada planejado.", "Nothing planned.")));
    }
    let mut edit = None;
    for w in planned {
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(w.display_title())
                        .strong()
                        .color(widgets::modality_color(w.modality)),
                );
                if w.activity_id.is_some() {
                    widgets::chip(ui, tr!("✔ feito", "✔ done"), widgets::GOOD);
                }
            });
            ui.label(weak(ui, summary(w)));
            if ui.small_button(tr!("✏ Abrir", "✏ Open")).clicked() {
                edit = Some(w.clone());
            }
        });
    }
    if ui.button(tr!("➕ Planejar neste dia", "➕ Plan on this day")).clicked() {
        edit = Some(PlannedWorkout::new(d));
    }
    if let Some(w) = edit {
        app.pages.calendar.editor = Some(Editor::new(w));
    }
    if let Some(id) = open {
        app.go(Page::Activity(id));
    }
}

/// One-line description of a planned workout's targets.
pub fn summary(w: &PlannedWorkout) -> String {
    let mut p = Vec::new();
    if let Some(d) = w.distance_m {
        p.push(format::km(d));
    }
    if let Some(t) = w.duration_s {
        p.push(format::duration(t));
    }
    if let Some(pc) = w.target_pace {
        p.push(format!(
            "{} = {} km/h",
            format::pace_unit(pc),
            format::num(format::pace_to_kmh(pc), 1)
        ));
    }
    if let Some(h) = w.target_hr {
        p.push(format!("≤ {} bpm", h.round()));
    }
    if let Some(z) = w.target_zone {
        p.push(format!("Z{z}"));
    }
    if !w.steps.is_empty() {
        let (t, d) = steps_totals(&w.steps);
        p.push(trf!(
            "{} blocos · ~{} · ~{}",
            "{} blocks · ~{} · ~{}",
            w.steps.len(),
            format::duration(t),
            format::km(d)
        ));
    }
    p.join(" · ")
}

fn editor_window(app: &mut App, ctx: &egui::Context) {
    let Some(ed) = app.pages.calendar.editor.as_mut() else {
        return;
    };
    let mut save = false;
    let mut delete = false;
    let mut open = ed.open;
    let tests = app.db.reference_tests().unwrap_or_default();
    let plans = app.db.plans().unwrap_or_default();
    let candidates: Vec<(i64, String)> = app
        .data
        .activities
        .iter()
        .filter(|a| (day_of(a) - ed.w.date).num_days().abs() <= 1)
        .map(|a| {
            (
                a.id,
                format!(
                    "{} {} · {}",
                    format::date_short(day_of(a)),
                    a.local_start().format("%H:%M"),
                    a.title()
                ),
            )
        })
        .collect();
    egui::Window::new(if ed.w.id == 0 {
        tr!("Planejar treino", "Plan a workout")
    } else {
        tr!("Treino planejado", "Planned workout")
    })
    .open(&mut open)
    .collapsible(false)
    .default_width(560.0)
    .max_height(ctx.content_rect().height() - 80.0)
    .show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("pw")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(tr!("Data", "Date"));
                    widgets::date_picker(ui, &mut ed.w.date, "pw-date");
                    ui.end_row();
                    ui.label(tr!("Modalidade", "Type"));
                    ui.horizontal_wrapped(|ui| {
                        for m in Modality::ALL {
                            ui.selectable_value(&mut ed.w.modality, m, m.label());
                        }
                    });
                    ui.end_row();
                    if ed.w.modality.is_run() {
                        ui.label(tr!("Tipo de treino", "Workout type"));
                        egui::ComboBox::from_id_salt("rt")
                            .selected_text(ed.w.run_type.map(|r| r.label()).unwrap_or("—"))
                            .show_ui(ui, |ui| {
                                for r in RunType::ALL {
                                    ui.selectable_value(&mut ed.w.run_type, Some(r), r.label());
                                }
                            });
                        ui.end_row();
                    }
                    ui.label(tr!("Título (opcional)", "Title (optional)"));
                    ui.text_edit_singleline(&mut ed.w.title);
                    ui.end_row();
                    ui.label(tr!("Plano", "Plan"));
                    egui::ComboBox::from_id_salt("pw-plan")
                        .selected_text(
                            ed.w.plan_id
                                .and_then(|id| plans.iter().find(|p| p.id == id))
                                .map(|p| p.name.as_str())
                                .unwrap_or("—"),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut ed.w.plan_id, None, "—");
                            for p in &plans {
                                ui.selectable_value(&mut ed.w.plan_id, Some(p.id), &p.name);
                            }
                        });
                    ui.end_row();
                    ui.label(tr!("Duração", "Duration"));
                    ui.add(
                        egui::TextEdit::singleline(&mut ed.duration)
                            .desired_width(80.0)
                            .hint_text(tr!("min ou h:mm:ss", "min or h:mm:ss")),
                    );
                    ui.end_row();
                    if ed.w.modality.is_run() {
                        ui.label(tr!("Distância (km)", "Distance (km)"));
                        ui.add(egui::TextEdit::singleline(&mut ed.distance).desired_width(80.0));
                        ui.end_row();
                        ui.label(tr!("Pace alvo", "Target pace"));
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut ed.pace)
                                    .desired_width(60.0)
                                    .hint_text("6:00"),
                            );
                            if let Some(p) = format::parse_pace(&ed.pace) {
                                ui.label(
                                    RichText::new(trf!(
                                        "esteira: {} km/h",
                                        "treadmill: {} km/h",
                                        format::num(format::pace_to_kmh(p), 1)
                                    ))
                                    .color(widgets::SPEED),
                                );
                            }
                        });
                        ui.end_row();
                    }
                    ui.label(tr!("FC alvo / zona", "Target HR / zone"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut ed.hr)
                                .desired_width(50.0)
                                .hint_text("bpm"),
                        );
                        for z in 1..=5 {
                            let sel = ed.w.target_zone == Some(z);
                            if ui.selectable_label(sel, format!("Z{z}")).clicked() {
                                ed.w.target_zone = if sel { None } else { Some(z) };
                            }
                        }
                    });
                    ui.end_row();
                    if ed.w.modality == Modality::Treadmill {
                        ui.label(tr!("Inclinação (%)", "Incline (%)"));
                        ui.add(egui::TextEdit::singleline(&mut ed.incline).desired_width(60.0));
                        ui.end_row();
                    }
                    if ed.w.run_type == Some(RunType::Test) && ed.w.modality.is_run() {
                        ui.label(tr!("Teste de referência", "Reference test"));
                        egui::ComboBox::from_id_salt("pw-test")
                            .selected_text(
                                ed.w.reference_test_id
                                    .and_then(|id| tests.iter().find(|t| t.id == id))
                                    .map(|t| t.name.as_str())
                                    .unwrap_or("—"),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut ed.w.reference_test_id, None, "—");
                                for t in &tests {
                                    ui.selectable_value(&mut ed.w.reference_test_id, Some(t.id), &t.name);
                                }
                            });
                        ui.end_row();
                    }
                    ui.label(tr!("Observações", "Notes"));
                    ui.add(
                        egui::TextEdit::multiline(&mut ed.w.notes)
                            .desired_rows(2)
                            .desired_width(320.0),
                    );
                    ui.end_row();
                    ui.label(tr!("Atividade realizada", "Done activity"));
                    egui::ComboBox::from_id_salt("pw-link")
                        .width(300.0)
                        .selected_text(
                            ed.w.activity_id
                                .and_then(|id| candidates.iter().find(|c| c.0 == id))
                                .map(|c| c.1.clone())
                                .unwrap_or_else(|| tr!("não vinculada", "not linked").into()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut ed.w.activity_id, None, tr!("não vinculada", "not linked"));
                            for (id, label) in &candidates {
                                ui.selectable_value(&mut ed.w.activity_id, Some(*id), label);
                            }
                        });
                    ui.end_row();
                });
            if ed.w.modality.is_run() {
                ui.separator();
                ui.label(RichText::new(tr!("Estrutura (intervalados)", "Structure (intervals)")).strong());
                steps_editor(ui, &mut ed.w.steps, &mut ed.bufs, "");
                ui.horizontal(|ui| {
                    if ui.button(tr!("➕ Bloco", "➕ Block")).clicked() {
                        ed.w.steps.push(Step::block(if ed.w.steps.is_empty() {
                            StepKind::Warmup
                        } else {
                            StepKind::Steady
                        }));
                    }
                    if ui.button(tr!("🔁 Repetição", "🔁 Repeat")).clicked() {
                        ed.w.steps.push(Step::Repeat {
                            times: 5,
                            steps: vec![Step::block(StepKind::Work), Step::block(StepKind::Recovery)],
                        });
                    }
                });
                if !ed.w.steps.is_empty() {
                    let (t, d) = steps_totals(&ed.w.steps);
                    ui.label(weak(
                        ui,
                        trf!(
                            "Total estimado: {} · {}",
                            "Estimated total: {} · {}",
                            format::duration(t),
                            format::km(d)
                        ),
                    ));
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                save = ui.button(RichText::new(tr!("💾 Salvar", "💾 Save")).strong()).clicked();
                if ed.w.id != 0
                    && ui
                        .button(RichText::new(tr!("🗑 Excluir", "🗑 Delete")).color(widgets::BAD))
                        .clicked()
                {
                    delete = true;
                }
            });
        });
    });
    ed.open = open;
    if save {
        ed.w.duration_s = format::parse_duration(&ed.duration).filter(|v| *v > 0.0);
        ed.w.distance_m = format::parse_num(&ed.distance).filter(|v| *v > 0.0).map(|v| v * 1000.0);
        ed.w.target_pace = format::parse_pace(&ed.pace);
        ed.w.target_hr = format::parse_num(&ed.hr).filter(|v| (60.0..230.0).contains(v));
        ed.w.incline_pct = format::parse_num(&ed.incline);
        if !ed.w.modality.is_run() {
            ed.w.run_type = None;
            ed.w.steps.clear();
        } else if ed.w.run_type.is_none() {
            ed.w.run_type = Some(RunType::Easy);
        }
        let w = ed.w.clone();
        match app.db.save_planned_workout(&w) {
            Ok(_) => {
                // a done test run joins the reference test history
                if let (Some(t), Some(a)) = (w.reference_test_id, w.activity_id) {
                    let _ = app.db.set_reference_run(t, a, true);
                }
                app.pages.calendar.editor = None;
                app.toast(tr!("Treino planejado salvo.", "Planned workout saved."));
            }
            Err(e) => app.toast_error(e.to_string()),
        }
    } else if delete {
        let id = ed.w.id;
        let _ = app.db.delete_planned_workout(id);
        app.pages.calendar.editor = None;
    } else if !open {
        app.pages.calendar.editor = None;
    }
}

/// Editor of a list of steps (blocks and repeats). `path` identifies the
/// buffers of nested steps.
fn steps_editor(
    ui: &mut Ui,
    steps: &mut Vec<Step>,
    bufs: &mut std::collections::HashMap<String, (String, String, String)>,
    path: &str,
) {
    let mut remove = None;
    let mut move_up = None;
    let mut duplicate = None;
    let n = steps.len();
    for (i, step) in steps.iter_mut().enumerate() {
        let key = format!("{path}{i}");
        match step {
            Step::Block {
                kind,
                duration_s,
                distance_m,
                pace,
                hr_zone,
                note,
            } => {
                let b = bufs.entry(key.clone()).or_insert_with(|| {
                    (
                        duration_s.map(format::duration).unwrap_or_default(),
                        distance_m.map(|d| format!("{}", d.round())).unwrap_or_default(),
                        pace.map(format::pace).unwrap_or_default(),
                    )
                });
                ui.horizontal(|ui| {
                    if !path.is_empty() {
                        ui.add_space(24.0);
                    }
                    egui::ComboBox::from_id_salt(format!("kind{key}"))
                        .width(110.0)
                        .selected_text(kind.label())
                        .show_ui(ui, |ui| {
                            for k in StepKind::ALL {
                                ui.selectable_value(kind, k, k.label());
                            }
                        });
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut b.0)
                                .hint_text(tr!("tempo", "time"))
                                .desired_width(54.0),
                        )
                        .changed()
                    {
                        *duration_s = format::parse_duration(&b.0).filter(|v| *v > 0.0);
                    }
                    ui.label(tr!("ou", "or"));
                    if ui
                        .add(egui::TextEdit::singleline(&mut b.1).hint_text("m").desired_width(50.0))
                        .changed()
                    {
                        *distance_m = format::parse_num(&b.1).filter(|v| *v > 0.0);
                    }
                    ui.label("@");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut b.2)
                                .hint_text("pace")
                                .desired_width(44.0),
                        )
                        .changed()
                    {
                        *pace = format::parse_pace(&b.2);
                    }
                    if let Some(p) = pace {
                        ui.label(
                            RichText::new(format!("{} km/h", format::num(format::pace_to_kmh(*p), 1)))
                                .color(widgets::SPEED)
                                .small(),
                        );
                    }
                    let z = hr_zone.map(|z| format!("Z{z}")).unwrap_or_else(|| "Z–".into());
                    egui::ComboBox::from_id_salt(format!("z{key}"))
                        .width(46.0)
                        .selected_text(z)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(hr_zone, None, "—");
                            for zz in 1..=5u8 {
                                ui.selectable_value(hr_zone, Some(zz), format!("Z{zz}"));
                            }
                        });
                    let _ = note;
                    row_buttons(ui, i, n, &mut remove, &mut move_up, &mut duplicate);
                });
            }
            Step::Repeat { times, steps: inner } => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr!("🔁 Repetir", "🔁 Repeat")).strong());
                    ui.add(egui::DragValue::new(times).range(1..=50).suffix("×"));
                    if ui.small_button(tr!("➕ bloco", "➕ block")).clicked() {
                        inner.push(Step::block(StepKind::Work));
                    }
                    row_buttons(ui, i, n, &mut remove, &mut move_up, &mut duplicate);
                });
                steps_editor(ui, inner, bufs, &format!("{key}."));
            }
        }
    }
    let changed = remove.is_some() || move_up.is_some() || duplicate.is_some();
    if let Some(i) = remove {
        steps.remove(i);
    }
    if let Some(i) = move_up {
        steps.swap(i, i - 1);
    }
    if let Some(i) = duplicate {
        let s = steps[i].clone();
        steps.insert(i + 1, s);
    }
    if changed {
        // indexes changed: rebuild the text buffers from the values
        bufs.retain(|k, _| !k.starts_with(path));
    }
}

fn row_buttons(
    ui: &mut Ui,
    i: usize,
    _n: usize,
    remove: &mut Option<usize>,
    up: &mut Option<usize>,
    dup: &mut Option<usize>,
) {
    if ui
        .add_enabled(i > 0, egui::Button::new("⬆").small())
        .on_hover_text(tr!("Subir", "Move up"))
        .clicked()
    {
        *up = Some(i);
    }
    if ui
        .small_button("⧉")
        .on_hover_text(tr!("Duplicar", "Duplicate"))
        .clicked()
    {
        *dup = Some(i);
    }
    if ui.small_button("✖").on_hover_text(tr!("Remover", "Remove")).clicked() {
        *remove = Some(i);
    }
}

/// The planned workout editor, for other pages (plans).
pub fn editor_window_public(app: &mut App, ctx: &egui::Context) {
    editor_window(app, ctx);
}
