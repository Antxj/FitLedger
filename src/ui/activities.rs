//! Activity history: sortable table with search and filters (item 4).
//! Also the entry point of the tennis and strength areas.

use super::widgets::{self, weak};
use super::{App, Page};
use crate::format;
use crate::model::{Activity, Modality};
use chrono::{Duration, NaiveDate};
use eframe::egui::{self, RichText, Ui};
use egui_extras::{Column, TableBuilder};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Date,
    Modality,
    Distance,
    Duration,
    Pace,
    AvgHr,
    MaxHr,
    Cadence,
    Ascent,
    Calories,
    Load,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Period {
    #[default]
    All,
    Days7,
    Days30,
    Months3,
    Months6,
    Year,
    Custom,
}

impl Period {
    fn label(self) -> &'static str {
        match self {
            Period::All => tr!("Tudo", "All"),
            Period::Days7 => tr!("7 dias", "7 days"),
            Period::Days30 => tr!("30 dias", "30 days"),
            Period::Months3 => tr!("3 meses", "3 months"),
            Period::Months6 => tr!("6 meses", "6 months"),
            Period::Year => tr!("1 ano", "1 year"),
            Period::Custom => tr!("Personalizado", "Custom"),
        }
    }

    pub fn days(self) -> Option<i64> {
        match self {
            Period::Days7 => Some(7),
            Period::Days30 => Some(30),
            Period::Months3 => Some(91),
            Period::Months6 => Some(182),
            Period::Year => Some(365),
            _ => None,
        }
    }
}

pub struct State {
    pub search: String,
    pub period: Period,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub min_km: String,
    pub max_km: String,
    pub min_hr: String,
    pub max_hr: String,
    pub sort: SortKey,
    pub descending: bool,
    pub selected: Vec<i64>,
}

impl Default for State {
    fn default() -> Self {
        let today = chrono::Local::now().date_naive();
        State {
            search: String::new(),
            period: Period::All,
            from: today - Duration::days(30),
            to: today,
            min_km: String::new(),
            max_km: String::new(),
            min_hr: String::new(),
            max_hr: String::new(),
            sort: SortKey::Date,
            descending: true,
            selected: Vec::new(),
        }
    }
}

fn matches(a: &Activity, s: &State, modality: Option<Modality>) -> bool {
    if let Some(m) = modality {
        if a.modality != m {
            return false;
        }
    }
    let day = a.local_start().date();
    let today = chrono::Local::now().date_naive();
    if let Some(d) = s.period.days() {
        if day < today - Duration::days(d - 1) {
            return false;
        }
    }
    if s.period == Period::Custom && (day < s.from || day > s.to) {
        return false;
    }
    let km = a.distance().unwrap_or(0.0) / 1000.0;
    if let Some(v) = format::parse_num(&s.min_km) {
        if km < v {
            return false;
        }
    }
    if let Some(v) = format::parse_num(&s.max_km) {
        if km > v {
            return false;
        }
    }
    if let Some(v) = format::parse_num(&s.min_hr) {
        if a.avg_hr.is_none_or(|h| h < v) {
            return false;
        }
    }
    if let Some(v) = format::parse_num(&s.max_hr) {
        if a.avg_hr.is_none_or(|h| h > v) {
            return false;
        }
    }
    let q = s.search.trim().to_lowercase();
    if !q.is_empty() {
        let hay = format!(
            "{} {} {} {} {}",
            a.title(),
            a.modality.label(),
            a.notes.as_deref().unwrap_or(""),
            a.device.as_deref().unwrap_or(""),
            format::date(day)
        )
        .to_lowercase();
        if !hay.contains(&q) {
            return false;
        }
    }
    true
}

fn sort_value(a: &Activity, key: SortKey) -> f64 {
    let o = |v: Option<f64>| v.unwrap_or(f64::NEG_INFINITY);
    match key {
        SortKey::Date => a.start_time as f64,
        SortKey::Modality => a.modality as u8 as f64,
        SortKey::Distance => o(a.distance()),
        SortKey::Duration => o(a.duration()),
        SortKey::Pace => o(a.pace_s_per_km()),
        SortKey::AvgHr => o(a.avg_hr),
        SortKey::MaxHr => o(a.max_hr),
        SortKey::Cadence => o(a.avg_cadence),
        SortKey::Ascent => o(a.ascent_m),
        SortKey::Calories => o(a.calories),
        SortKey::Load => o(a.load),
    }
}

pub fn show(app: &mut App, ui: &mut Ui, modality: Option<Modality>) {
    let title = match modality {
        None => tr!("Atividades", "Activities"),
        Some(m) => m.label(),
    };
    ui.horizontal(|ui| {
        widgets::page_title(ui, title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let n = app.pages.activities.selected.len();
            if ui
                .add_enabled(n >= 2, egui::Button::new(trf!("Comparar ({n})", "Compare ({n})")))
                .on_disabled_hover_text(tr!("Marque 2 ou mais atividades na tabela.", "Tick 2 or more activities in the table."))
                .clicked()
            {
                app.compare_ids = app.pages.activities.selected.clone();
                app.go(Page::Compare);
            }
            if ui.button(tr!("🔄 Procurar novos em /fits", "🔄 Look for new files in /fits")).clicked() {
                app.import_from_folder(false);
            }
        });
    });

    match modality {
        Some(Modality::Tennis) => {
            egui::CollapsingHeader::new(RichText::new(tr!("Visão geral do tênis", "Tennis overview")).strong())
                .default_open(true)
                .show(ui, |ui| super::tennis::overview(app, ui));
        }
        Some(Modality::Strength) => {
            egui::CollapsingHeader::new(RichText::new(tr!("Musculação: exercícios e evolução", "Strength: exercises and progress")).strong())
                .default_open(true)
                .show(ui, |ui| super::strength::overview(app, ui));
        }
        _ => {}
    }

    filters(ui, &mut app.pages.activities);
    ui.add_space(6.0);

    let s = &app.pages.activities;
    let mut rows: Vec<&Activity> = app.data.activities.iter().filter(|a| matches(a, s, modality)).collect();
    let (key, desc) = (s.sort, s.descending);
    rows.sort_by(|a, b| {
        let o = sort_value(a, key).partial_cmp(&sort_value(b, key)).unwrap_or(std::cmp::Ordering::Equal);
        if desc { o.reverse() } else { o }
    });

    if rows.is_empty() {
        widgets::empty_state(
            ui,
            if app.data.activities.is_empty() {
                tr!(
                    "Nenhuma atividade ainda. Coloque arquivos .fit na pasta /fits ou use \"Importar arquivos FIT\".",
                    "No activities yet. Put .fit files in the /fits folder or use \"Import FIT files\"."
                )
            } else {
                tr!("Nenhuma atividade com esses filtros.", "No activities match these filters.")
            },
        );
        return;
    }
    let total_km: f64 = rows.iter().filter_map(|a| a.distance()).sum::<f64>() / 1000.0;
    let total_s: f64 = rows.iter().filter_map(|a| a.duration()).sum();
    ui.label(weak(
        ui,
        trf!(
            "{} atividades · {} km · {}",
            "{} activities · {} km · {}",
            rows.len(),
            format::num(total_km, 1),
            format::hours_short(total_s)
        ),
    ));

    let ids: Vec<i64> = rows.iter().map(|a| a.id).collect();
    let mut open: Option<i64> = None;
    let mut toggle: Option<i64> = None;
    let mut sort_click: Option<SortKey> = None;
    let selected = app.pages.activities.selected.clone();
    let header = |ui: &mut Ui, label: &str, k: SortKey, click: &mut Option<SortKey>| {
        let arrow = if key == k { if desc { " ▼" } else { " ▲" } } else { "" };
        if ui.add(egui::Button::new(RichText::new(format!("{label}{arrow}")).strong()).frame(false)).clicked() {
            *click = Some(k);
        }
    };
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::exact(24.0))
        .column(Column::initial(112.0).at_least(90.0))
        .column(Column::initial(104.0).at_least(70.0))
        .columns(Column::initial(66.0).at_least(46.0).clip(true), 9)
        .column(Column::remainder().at_least(40.0).clip(true))
        .header(26.0, |mut h| {
            h.col(|_| {});
            h.col(|ui| header(ui, tr!("Data", "Date"), SortKey::Date, &mut sort_click));
            h.col(|ui| header(ui, tr!("Modalidade", "Type"), SortKey::Modality, &mut sort_click));
            h.col(|ui| header(ui, tr!("Distância", "Distance"), SortKey::Distance, &mut sort_click));
            h.col(|ui| header(ui, tr!("Duração", "Duration"), SortKey::Duration, &mut sort_click));
            h.col(|ui| header(ui, "Pace", SortKey::Pace, &mut sort_click));
            h.col(|ui| header(ui, tr!("FC méd", "Avg HR"), SortKey::AvgHr, &mut sort_click));
            h.col(|ui| header(ui, tr!("FC máx", "Max HR"), SortKey::MaxHr, &mut sort_click));
            h.col(|ui| header(ui, tr!("Cadência", "Cadence"), SortKey::Cadence, &mut sort_click));
            h.col(|ui| header(ui, tr!("Elevação", "Elevation"), SortKey::Ascent, &mut sort_click));
            h.col(|ui| header(ui, tr!("Calorias", "Calories"), SortKey::Calories, &mut sort_click));
            h.col(|ui| header(ui, tr!("Carga", "Load"), SortKey::Load, &mut sort_click));
            h.col(|ui| {
                ui.label(RichText::new(tr!("Observações", "Notes")).strong());
            });
        })
        .body(|body| {
            body.rows(26.0, rows.len(), |mut row| {
                let a = rows[row.index()];
                row.set_selected(selected.contains(&a.id));
                row.col(|ui| {
                    let mut on = selected.contains(&a.id);
                    if ui.checkbox(&mut on, "").changed() {
                        toggle = Some(a.id);
                    }
                });
                row.col(|ui| {
                    ui.label(format::datetime(a.local_start()));
                });
                row.col(|ui| {
                    ui.label(RichText::new(a.modality.label()).color(widgets::modality_color(a.modality)));
                });
                row.col(|ui| {
                    ui.label(format::opt_km(a.distance()));
                });
                row.col(|ui| {
                    ui.label(format::opt_duration(a.duration()));
                });
                row.col(|ui| {
                    ui.label(if a.modality.is_run() { format::opt_pace(a.pace_s_per_km()) } else { format::dash() });
                });
                row.col(|ui| {
                    ui.label(format::bpm(a.avg_hr));
                });
                row.col(|ui| {
                    ui.label(format::bpm(a.max_hr));
                });
                row.col(|ui| {
                    ui.label(a.avg_cadence.map(|c| format!("{} spm", c.round())).unwrap_or_else(format::dash));
                });
                row.col(|ui| {
                    ui.label(a.ascent_m.map(|v| format!("{} m", v.round())).unwrap_or_else(format::dash));
                });
                row.col(|ui| {
                    ui.label(a.calories.map(|v| format!("{} kcal", v.round())).unwrap_or_else(format::dash));
                });
                row.col(|ui| {
                    ui.label(a.load.map(|v| format!("{}", v.round())).unwrap_or_else(format::dash));
                });
                row.col(|ui| {
                    let n = a.notes.as_deref().unwrap_or("").lines().next().unwrap_or("");
                    ui.label(weak(ui, n));
                });
                if row.response().clicked() {
                    open = Some(a.id);
                }
            });
        });

    if let Some(k) = sort_click {
        let st = &mut app.pages.activities;
        if st.sort == k {
            st.descending = !st.descending;
        } else {
            st.sort = k;
            st.descending = true;
        }
    }
    if let Some(id) = toggle {
        let sel = &mut app.pages.activities.selected;
        if let Some(i) = sel.iter().position(|x| *x == id) {
            sel.remove(i);
        } else {
            sel.push(id);
        }
    }
    // drop selections that are no longer visible in the list
    app.pages.activities.selected.retain(|id| ids.contains(id) || app.data.get(*id).is_some());
    if let Some(id) = open {
        app.go(Page::Activity(id));
    }
}

fn filters(ui: &mut Ui, s: &mut State) {
    ui.horizontal_wrapped(|ui| {
        ui.add(egui::TextEdit::singleline(&mut s.search).hint_text(tr!("🔍 Pesquisar", "🔍 Search")).desired_width(170.0));
        ui.separator();
        egui::ComboBox::from_id_salt("period")
            .selected_text(s.period.label())
            .show_ui(ui, |ui| {
                for p in [Period::All, Period::Days7, Period::Days30, Period::Months3, Period::Months6, Period::Year, Period::Custom] {
                    ui.selectable_value(&mut s.period, p, p.label());
                }
            });
        if s.period == Period::Custom {
            widgets::date_picker(ui, &mut s.from, "from");
            ui.label("→");
            widgets::date_picker(ui, &mut s.to, "to");
        }
        ui.separator();
        ui.label(tr!("km", "km"));
        ui.add(egui::TextEdit::singleline(&mut s.min_km).hint_text("min").desired_width(40.0));
        ui.add(egui::TextEdit::singleline(&mut s.max_km).hint_text("máx").desired_width(40.0));
        ui.separator();
        ui.label(tr!("FC méd", "Avg HR"));
        ui.add(egui::TextEdit::singleline(&mut s.min_hr).hint_text("min").desired_width(40.0));
        ui.add(egui::TextEdit::singleline(&mut s.max_hr).hint_text("máx").desired_width(40.0));
        if ui.button(tr!("Limpar", "Clear")).clicked() {
            let keep = std::mem::take(&mut s.selected);
            *s = State { selected: keep, ..Default::default() };
        }
    });
}
