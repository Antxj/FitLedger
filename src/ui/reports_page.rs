//! Reports: an HTML report for a period, and CSV/JSON exports.

use super::App;
use super::widgets::{self, weak};
use crate::coach::fitness::day;
use crate::format;
use crate::model::Activity;
use crate::report::{self, Period};
use eframe::egui::{self, RichText, Ui};

pub struct State {
    period: Period,
}

impl Default for State {
    fn default() -> Self {
        State {
            period: Period::SinceGoal,
        }
    }
}

fn save(app: &mut App, name: &str, ext: &str, label: &str, text: String, open: bool) {
    let Some(dest) = rfd::FileDialog::new()
        .set_file_name(name)
        .add_filter(label, &[ext])
        .save_file()
    else {
        return;
    };
    match std::fs::write(&dest, text) {
        Ok(()) => {
            app.toast(trf!("Arquivo salvo: {}", "File saved: {}", dest.display()));
            if open {
                let _ = std::process::Command::new("explorer").arg(&dest).spawn();
            }
        }
        Err(e) => app.toast_error(e.to_string()),
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    widgets::page_title(ui, tr!("Relatórios", "Reports"));
    let today = chrono::Local::now().date_naive();
    let first = app.data.activities.iter().map(day).min();
    ui.horizontal(|ui| {
        ui.label(tr!("Período:", "Period:"));
        for p in Period::ALL {
            ui.selectable_value(&mut app.pages.reports.period, p, p.label());
        }
    });
    let from = app.pages.reports.period.from(today, app.coach.as_ref(), first);
    let owned: Vec<Activity> = app
        .data
        .activities
        .iter()
        .filter(|a| day(a) >= from && day(a) <= today)
        .cloned()
        .collect();
    let runs: Vec<&Activity> = owned.iter().collect();
    let km: f64 = runs.iter().filter_map(|a| a.distance()).sum::<f64>() / 1000.0;
    let secs: f64 = runs.iter().filter_map(|a| a.duration()).sum();
    ui.label(weak(
        ui,
        trf!(
            "{} – {}: {} corridas · {} km · {}",
            "{} – {}: {} runs · {} km · {}",
            format::date(from),
            format::date(today),
            runs.len(),
            format::num(km, 1),
            format::hours_short(secs)
        ),
    ));
    ui.add_space(8.0);
    let stamp = today.format("%Y-%m-%d").to_string();
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(tr!("Relatório completo (HTML)", "Full report (HTML)")).size(16.0).strong());
        ui.label(weak(
            ui,
            tr!(
                "Meta e estimativa, FC no pace da meta, volume semanal, aderência ao plano, pace e deriva ao longo do tempo, tabela de corridas e observações do treinador. Abre no navegador; para PDF, use Imprimir → Salvar como PDF.",
                "Goal and estimate, HR at goal pace, weekly volume, plan adherence, pace and drift over time, runs table and coach notes. Opens in the browser; for a PDF, use Print → Save as PDF."
            ),
        ));
        if ui.button(RichText::new(tr!("📄 Gerar e abrir relatório", "📄 Generate and open report")).strong()).clicked() {
            let all: Vec<Activity> = app.data.activities.clone();
            let html = report::html(&all, &app.data.metrics, app.coach.as_ref(), from, today);
            save(app, &format!("FitLedger-report-{stamp}.html"), "html", "HTML", html, true);
        }
    });
    ui.add_space(8.0);
    widgets::card_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(tr!("Exportar dados", "Export data")).size(16.0).strong());
        ui.label(weak(
            ui,
            tr!(
                "CSV abre no Excel/Planilhas; JSON serve para outros programas.",
                "CSV opens in Excel/Sheets; JSON is for other programs."
            ),
        ));
        let mut sorted = runs.clone();
        sorted.sort_by_key(|a| a.start_time);
        let tables: Vec<(&str, &str, report::Table)> = {
            let mut v = vec![(
                tr!("Corridas", "Runs"),
                "runs",
                report::runs_table(&sorted, &app.data.metrics, app.coach.as_ref()),
            )];
            if let Some(c) = &app.coach {
                v.push((
                    tr!("Plano (planejado × feito)", "Plan (planned vs done)"),
                    "plan",
                    report::plan_table(c),
                ));
            }
            let w = report::weeks(&runs, app.coach.as_ref(), from, today);
            v.push((
                tr!("Resumo semanal", "Weekly summary"),
                "weekly",
                report::weeks_table(&w),
            ));
            v
        };
        let mut action = None;
        egui::Grid::new("exports")
            .num_columns(3)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                for (i, (label, _, t)) in tables.iter().enumerate() {
                    ui.label(format!("{label} ({})", t.rows.len()));
                    if ui.button("⬇ CSV").clicked() {
                        action = Some((i, "csv"));
                    }
                    if ui.button("⬇ JSON").clicked() {
                        action = Some((i, "json"));
                    }
                    ui.end_row();
                }
            });
        if let Some((i, ext)) = action {
            let (_, stem, t) = &tables[i];
            let text = if ext == "csv" {
                report::to_csv(t)
            } else {
                report::to_json(t)
            };
            let name = format!("fitledger-{stem}-{stamp}.{ext}");
            save(app, &name, ext, &ext.to_uppercase(), text, false);
        }
    });
}
