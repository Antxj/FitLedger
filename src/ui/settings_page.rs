//! Settings (items 7, 31, 32, 33, 37): profile, heart rate and zones,
//! units, theme, language, updates, backup/restore and recalculation.

use super::widgets::{self, weak};
use super::{App, JobKind};
use crate::backup;
use crate::db::Db;
use crate::format;
use crate::i18n::Idioma;
use crate::settings::{Theme, ZoneMethod};
use eframe::egui::{self, RichText, Ui};

#[derive(Default)]
pub struct State {
    loaded: bool,
    age: String,
    weight: String,
    height: String,
    hr_max: String,
    hr_rest: String,
    zones: [String; 5],
    confirm_restore: bool,
    restore_src: Option<std::path::PathBuf>,
    restore_info: String,
}

fn opt(v: Option<f64>) -> String {
    v.map(|v| format::num_trim(v, 1)).unwrap_or_default()
}

pub fn show(app: &mut App, ui: &mut Ui) {
    if !app.pages.settings.loaded {
        let st = &app.settings;
        let s = &mut app.pages.settings;
        s.age = st.age.map(|a| a.to_string()).unwrap_or_default();
        s.weight = opt(st.weight_kg);
        s.height = opt(st.height_cm);
        s.hr_max = opt(st.hr_max);
        s.hr_rest = opt(st.hr_rest);
        s.zones = st.custom_zones.map(|z| format::num_trim(z, 0));
        s.loaded = true;
    }
    widgets::page_title(ui, tr!("Configurações", "Settings"));
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.set_max_width(820.0);
        profile(app, ui);
        heart_rate(app, ui);
        appearance(app, ui);
        data(app, ui);
        updates(app, ui);
    });
    let ctx = ui.ctx().clone();
    let text = app.pages.settings.restore_info.clone();
    if widgets::confirm_window(
        &ctx,
        &mut app.pages.settings.confirm_restore,
        tr!("Restaurar backup", "Restore backup"),
        &text,
    ) && let Some(src) = app.pages.settings.restore_src.take()
    {
        restore(app, &src);
    }
}

fn profile(app: &mut App, ui: &mut Ui) {
    widgets::section(ui, tr!("Dados pessoais (opcionais)", "Personal data (optional)"));
    let s = &mut app.pages.settings;
    let mut changed = false;
    egui::Grid::new("profile")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            let mut field = |ui: &mut Ui, label: &str, buf: &mut String, unit: &str| {
                ui.label(label);
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::TextEdit::singleline(buf).desired_width(80.0)).changed();
                    ui.label(unit);
                });
                ui.end_row();
            };
            field(ui, tr!("Idade", "Age"), &mut s.age, tr!("anos", "years"));
            field(ui, tr!("Peso", "Weight"), &mut s.weight, "kg");
            field(ui, tr!("Altura", "Height"), &mut s.height, "cm");
        });
    if changed {
        app.settings.age = format::parse_num(&s.age)
            .filter(|v| (5.0..120.0).contains(v))
            .map(|v| v as u32);
        app.settings.weight_kg = format::parse_num(&s.weight).filter(|v| *v > 20.0);
        app.settings.height_cm = format::parse_num(&s.height).filter(|v| *v > 80.0);
        app.save_settings();
    }
}

fn heart_rate(app: &mut App, ui: &mut Ui) {
    widgets::section(ui, tr!("Frequência cardíaca e zonas", "Heart rate and zones"));
    let observed = app.db.observed_max_hr();
    let s = &mut app.pages.settings;
    let mut changed = false;
    egui::Grid::new("hr")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label(tr!("FC máxima", "Max HR"));
            ui.horizontal(|ui| {
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut s.hr_max).desired_width(80.0))
                    .changed();
                ui.label("bpm");
                ui.label(weak(
                    ui,
                    trf!(
                        "em branco: usa a maior registrada ({}) ou 220 − idade",
                        "blank: uses the highest recorded ({}) or 220 − age",
                        format::bpm(observed)
                    ),
                ));
            });
            ui.end_row();
            ui.label(tr!("FC de repouso", "Resting HR"));
            ui.horizontal(|ui| {
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut s.hr_rest).desired_width(80.0))
                    .changed();
                ui.label("bpm");
                ui.label(weak(
                    ui,
                    tr!(
                        "usada na carga (TRIMP) e nas zonas por reserva",
                        "used for load (TRIMP) and reserve zones"
                    ),
                ));
            });
            ui.end_row();
            ui.label(tr!("Método das zonas", "Zone method"));
            ui.horizontal(|ui| {
                let m = &mut app.settings.zone_method;
                changed |= ui
                    .selectable_value(m, ZoneMethod::MaxHr, tr!("% da FC máxima", "% of max HR"))
                    .changed();
                changed |= ui
                    .selectable_value(
                        m,
                        ZoneMethod::Reserve,
                        tr!("% da reserva (Karvonen)", "% of reserve (Karvonen)"),
                    )
                    .changed();
                changed |= ui
                    .selectable_value(m, ZoneMethod::Custom, tr!("Personalizadas", "Custom"))
                    .changed();
            });
            ui.end_row();
            if app.settings.zone_method == ZoneMethod::Custom {
                ui.label(tr!("Início de cada zona (bpm)", "Start of each zone (bpm)"));
                ui.horizontal(|ui| {
                    for (i, z) in s.zones.iter_mut().enumerate() {
                        ui.label(format!("Z{}", i + 1));
                        changed |= ui.add(egui::TextEdit::singleline(z).desired_width(44.0)).changed();
                    }
                });
                ui.end_row();
            }
        });
    if changed {
        app.settings.hr_max = format::parse_num(&s.hr_max).filter(|v| (120.0..240.0).contains(v));
        app.settings.hr_rest = format::parse_num(&s.hr_rest).filter(|v| (25.0..120.0).contains(v));
        for (i, z) in s.zones.iter().enumerate() {
            if let Some(v) = format::parse_num(z) {
                app.settings.custom_zones[i] = v;
            }
        }
        app.save_settings();
    }
    let zones = app.settings.zones(observed);
    ui.add_space(4.0);
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
    ui.label(weak(ui, tr!(
        "Depois de mudar as zonas ou a FC, use \"Recalcular métricas\" (abaixo) para atualizar os treinos antigos.",
        "After changing zones or HR, use \"Recalculate metrics\" (below) to update past workouts."
    )).small());
}

fn appearance(app: &mut App, ui: &mut Ui) {
    widgets::section(
        ui,
        tr!("Aparência, idioma e unidades", "Appearance, language and units"),
    );
    ui.horizontal(|ui| {
        ui.label(tr!("Tema:", "Theme:"));
        let mut changed = ui
            .selectable_value(
                &mut app.settings.theme,
                Theme::System,
                tr!("Automático (Windows)", "Automatic (Windows)"),
            )
            .changed();
        changed |= ui
            .selectable_value(&mut app.settings.theme, Theme::Dark, tr!("Escuro", "Dark"))
            .changed();
        changed |= ui
            .selectable_value(&mut app.settings.theme, Theme::Light, tr!("Claro", "Light"))
            .changed();
        if changed {
            app.save_settings();
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr!("Idioma:", "Language:"));
        let current = app.settings.language.clone();
        if ui
            .selectable_label(current.is_none(), tr!("Automático (Windows)", "Automatic (Windows)"))
            .clicked()
        {
            app.settings.language = None;
            crate::i18n::aplicar(None);
            app.save_settings();
        }
        for i in [Idioma::Portugues, Idioma::Ingles] {
            if ui
                .selectable_label(current.as_deref() == Some(i.codigo()), i.nome())
                .clicked()
            {
                app.settings.language = Some(i.codigo().into());
                crate::i18n::aplicar(Some(i));
                app.save_settings();
            }
        }
    });
    ui.label(weak(
        ui,
        tr!(
            "Unidades: distância em km, pace em min/km, velocidade em km/h, peso em kg e temperatura em °C.",
            "Units: distance in km, pace in min/km, speed in km/h, weight in kg and temperature in °C."
        ),
    ));
}

fn data(app: &mut App, ui: &mut Ui) {
    widgets::section(ui, tr!("Dados", "Data"));
    ui.label(weak(
        ui,
        trf!("Pasta do aplicativo: {}", "App folder: {}", app.paths.base.display()),
    ));
    ui.label(weak(
        ui,
        trf!("Arquivos FIT: {}", "FIT files: {}", app.paths.fits.display()),
    ));
    ui.label(weak(
        ui,
        trf!("Banco de dados: {}", "Database: {}", app.paths.db.display()),
    ));
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr!("📁 Abrir pasta /fits", "📁 Open /fits folder")).clicked() {
            let _ = std::process::Command::new("explorer").arg(&app.paths.fits).spawn();
        }
        if ui
            .button(tr!("📁 Abrir pasta de dados", "📁 Open data folder"))
            .clicked()
        {
            let _ = std::process::Command::new("explorer").arg(&app.paths.data).spawn();
        }
        let mut auto = app.settings.auto_import;
        if ui
            .checkbox(
                &mut auto,
                tr!(
                    "Importar novos arquivos de /fits ao abrir",
                    "Import new files from /fits on start"
                ),
            )
            .changed()
        {
            app.settings.auto_import = auto;
            app.save_settings();
        }
    });
    let failed = crate::import::KnownErrors::load(&app.db);
    if !failed.0.is_empty() {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(trf!(
                    "{} arquivo(s) em /fits falharam e não são tentados de novo automaticamente.",
                    "{} file(s) in /fits failed and are not retried automatically.",
                    failed.0.len()
                ))
                .color(widgets::WARN),
            );
            if ui.button(tr!("Tentar de novo", "Try again")).clicked() {
                crate::import::KnownErrors::default().save(&app.db);
                app.import_from_folder(false);
            }
        });
        for (name, err) in failed.0.values() {
            ui.label(weak(ui, format!("• {name}: {err}")).small());
        }
    }
    ui.add_space(6.0);
    let busy = app.job_running();
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(!busy, egui::Button::new(tr!("🔁 Recalcular métricas", "🔁 Recalculate metrics")))
            .on_hover_text(tr!(
                "Recalcula splits, zonas, deriva cardíaca, pace × FC, carga, records e insights a partir dos dados já importados, sem reimportar os arquivos.",
                "Recalculates splits, zones, cardiac drift, pace × HR, load, records and insights from the data already imported, without re-importing files."
            ))
            .clicked()
        {
            app.start_job(JobKind::Recalculate);
        }
        if ui.button(tr!("💾 Criar backup", "💾 Create backup")).clicked()
            && let Some(dest) = rfd::FileDialog::new()
                .set_title(tr!("Criar backup", "Create backup"))
                .set_file_name(backup::default_file_name())
                .add_filter("FitLedger", &[backup::EXTENSION])
                .save_file()
            {
                match backup::create(&app.db, &dest) {
                    Ok(()) => app.toast(trf!("Backup criado: {}", "Backup created: {}", dest.display())),
                    Err(e) => app.toast_error(trf!("Falha no backup: {e}", "Backup failed: {e}")),
                }
            }
        if ui.add_enabled(!busy, egui::Button::new(tr!("♻ Restaurar backup", "♻ Restore backup"))).clicked()
            && let Some(src) = rfd::FileDialog::new()
                .set_title(tr!("Restaurar backup", "Restore backup"))
                .add_filter("FitLedger", &[backup::EXTENSION, "db"])
                .pick_file()
            {
                match backup::inspect(&src) {
                    Ok(info) => {
                        app.pages.settings.restore_info = trf!(
                            "O backup tem {} atividades ({} arquivos FIT). Os dados atuais serão substituídos por ele. \
                             Uma cópia dos dados atuais fica guardada em data\\backups antes da troca. Continuar?",
                            "The backup has {} activities ({} FIT files). The current data will be replaced by it. \
                             A copy of the current data is kept in data\\backups before the swap. Continue?",
                            info.activities,
                            info.fit_files
                        );
                        app.pages.settings.restore_src = Some(src);
                        app.pages.settings.confirm_restore = true;
                    }
                    Err(e) => app.toast_error(trf!("Não é possível restaurar: {e}", "Cannot restore: {e}")),
                }
            }
    });
    ui.label(
        weak(
            ui,
            tr!(
                "Relatórios e exportação em CSV/JSON ficam na página Relatórios.",
                "Reports and CSV/JSON export are on the Reports page."
            ),
        )
        .small(),
    );
}

fn restore(app: &mut App, src: &std::path::Path) {
    // close our connection, swap the file, reopen
    let placeholder = match Db::open_in_memory() {
        Ok(d) => d,
        Err(e) => return app.toast_error(e.to_string()),
    };
    drop(std::mem::replace(&mut app.db, placeholder));
    let result = backup::restore(src, &app.paths.db, &app.paths.backups);
    match Db::open(&app.paths.db, Some(&app.paths.backups)) {
        Ok(db) => app.db = db,
        Err(e) => {
            return app.toast_error(trf!(
                "Erro ao reabrir o banco: {e}",
                "Error reopening the database: {e}"
            ));
        }
    }
    match result {
        Ok(()) => {
            app.settings = app.db.load_settings();
            app.settings.onboarded = true;
            app.pages = super::PageStates::default();
            app.reload();
            if crate::analysis::metrics::needs_recalculation(&app.db) {
                app.start_job(JobKind::Recalculate);
            }
            app.toast(tr!("Backup restaurado.", "Backup restored."));
        }
        Err(e) => app.toast_error(trf!("Falha ao restaurar: {e}", "Restore failed: {e}")),
    }
}

fn updates(app: &mut App, ui: &mut Ui) {
    use crate::update::{self, Estado};
    widgets::section(ui, tr!("Atualizações", "Updates"));
    let mut auto = app.settings.auto_update;
    if ui
        .checkbox(
            &mut auto,
            tr!(
                "Procurar novas versões automaticamente (uma vez por dia)",
                "Check for new versions automatically (once a day)"
            ),
        )
        .changed()
    {
        app.settings.auto_update = auto;
        app.save_settings();
    }
    ui.label(weak(ui, tr!(
        "A única conexão de rede do FitLedger é esta consulta à página de versões do GitHub. Nenhum dado de treino sai do computador.",
        "FitLedger's only network connection is this query to the GitHub releases page. No workout data leaves the computer."
    )).small());
    ui.horizontal(|ui| {
        if ui.button(tr!("Verificar agora", "Check now")).clicked() {
            update::verificar_agora();
        }
        match update::estado() {
            Estado::Verificando => {
                ui.spinner();
            }
            Estado::EmDia => {
                ui.label(
                    RichText::new(trf!(
                        "Você está na versão mais recente (v{}).",
                        "You have the latest version (v{}).",
                        update::VERSAO_ATUAL
                    ))
                    .color(widgets::GOOD),
                );
            }
            Estado::FalhaVerificacao(e) => {
                ui.label(RichText::new(e).color(widgets::WARN));
            }
            Estado::Disponivel(v) => {
                if v.instalavel()
                    && ui
                        .button(RichText::new(trf!("⬆ Instalar v{}", "⬆ Install v{}", v.numero)).strong())
                        .clicked()
                {
                    update::instalar(v.clone());
                }
                ui.hyperlink_to(tr!("ver novidades", "see what's new"), &v.pagina);
            }
            _ => {}
        }
        ui.hyperlink_to("GitHub", update::pagina_repositorio());
    });
}
