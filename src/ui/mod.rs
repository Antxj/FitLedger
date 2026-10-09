//! User interface (egui). `App` owns the database connection and a cache of
//! the activities; each page lives in its own module.

mod activities;
mod activity;
mod analysis;
mod calendar;
pub mod charts;
mod compare;
mod dashboard;
mod evolution;
mod goals;
mod insights_page;
mod manual;
mod onboarding;
mod plans;
mod records;
mod settings_page;
mod strength;
mod tennis;
pub mod widgets;

use crate::analysis::metrics::{self, Metrics};
use crate::db::Db;
use crate::import;
use crate::model::{Activity, Modality};
use crate::paths::Paths;
use crate::settings::{Settings, Theme};
use eframe::egui::{self, Color32, RichText};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const APP_NAME: &str = "FitLedger";

#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    Onboarding,
    Dashboard,
    Activities(Option<Modality>),
    Activity(i64),
    Manual(Option<i64>),
    Compare,
    Calendar,
    Plans,
    Goals,
    Evolution,
    Records,
    Analysis,
    Insights,
    Settings,
}

/// Cache of everything the pages read often. Rebuilt after any change.
#[derive(Default)]
pub struct Data {
    pub activities: Vec<Activity>,
    pub metrics: HashMap<i64, Metrics>,
    /// Incremented on every reload: pages use it to refresh their own caches.
    pub generation: u64,
}

impl Data {
    pub fn get(&self, id: i64) -> Option<&Activity> {
        self.activities.iter().find(|a| a.id == id)
    }

    pub fn metrics(&self, id: i64) -> Option<&Metrics> {
        self.metrics.get(&id)
    }
}

#[derive(Debug, Clone)]
pub enum JobKind {
    Import {
        files: Vec<std::path::PathBuf>,
        remember_errors: bool,
    },
    Recalculate,
}

#[derive(Default)]
pub struct JobState {
    pub running: bool,
    pub label: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
    pub finished: Option<JobResult>,
}

pub enum JobResult {
    Import(import::Report),
    Recalculate(usize),
    Failed(String),
}

pub struct Toast {
    pub text: String,
    pub error: bool,
    pub until: Instant,
}

pub struct App {
    pub db: Db,
    pub paths: Paths,
    pub settings: Settings,
    pub data: Data,
    pub page: Page,
    history: Vec<Page>,
    pub job: Arc<Mutex<JobState>>,
    pub toasts: Vec<Toast>,
    /// Last import report (shown in the import dialog).
    pub last_report: Option<import::Report>,
    pub show_report: bool,
    pub compare_ids: Vec<i64>,
    pub pages: PageStates,
    applied_theme: Option<Theme>,
    shooter: Option<crate::screenshots::Shooter>,
    shots: Vec<(String, Page, Option<activity::Tab>)>,
}

/// Per-page UI state that must survive between frames.
#[derive(Default)]
pub struct PageStates {
    pub activities: activities::State,
    pub activity: activity::State,
    pub manual: manual::State,
    pub analysis: analysis::State,
    pub calendar: calendar::State,
    pub plans: plans::State,
    pub goals: goals::State,
    pub evolution: evolution::State,
    pub strength: strength::State,
    pub settings: settings_page::State,
    pub onboarding: onboarding::State,
    pub compare: compare::State,
    pub insights: insights_page::State,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, db: Db, paths: Paths) -> App {
        let settings = db.load_settings();
        crate::i18n::aplicar(settings.language.as_deref().and_then(crate::i18n::Idioma::do_codigo));
        widgets::setup_style(&cc.egui_ctx);
        crate::single::spawn_show_listener(cc.egui_ctx.clone());

        let first_run = !settings.onboarded;
        let mut app = App {
            db,
            paths,
            settings,
            data: Data::default(),
            page: if first_run { Page::Onboarding } else { Page::Dashboard },
            history: Vec::new(),
            job: Arc::new(Mutex::new(JobState::default())),
            toasts: Vec::new(),
            last_report: None,
            show_report: false,
            compare_ids: Vec::new(),
            pages: PageStates::default(),
            applied_theme: None,
            shooter: crate::screenshots::Shooter::from_env(),
            shots: Vec::new(),
        };
        app.reload();
        if app.shooter.is_some() {
            // screenshot mode: import synchronously, no network
            if std::env::var_os("FITLEDGER_DEMO").is_some()
                && let Err(e) = crate::demo::fill(&mut app.db)
            {
                eprintln!("demo data: {e}");
            }
            if app.data.activities.is_empty() {
                let files = import::scan(&app.paths.fits);
                import::import_files(&mut app.db, &files, false, &mut |_, _, _| {});
            }
            app.settings.onboarded = true;
            app.save_settings();
            app.reload();
            app.shots = app.screenshot_plan();
            return app;
        }

        if !first_run {
            if metrics::needs_recalculation(&app.db) && !app.data.activities.is_empty() {
                app.start_job(JobKind::Recalculate);
            } else if app.settings.auto_import {
                app.import_from_folder(true);
            }
        }
        // the periodic check always runs, but only queries GitHub while enabled
        AUTO_UPDATE.store(app.settings.auto_update, std::sync::atomic::Ordering::Relaxed);
        let ctx = cc.egui_ctx.clone();
        crate::update::ao_mudar(move || ctx.request_repaint());
        crate::update::iniciar_verificacao_periodica(auto_update_enabled);
        app
    }

    /// Reloads the activity cache from the database.
    pub fn reload(&mut self) {
        match self.db.activities() {
            Ok(a) => self.data.activities = a,
            Err(e) => self.toast_error(trf!("Erro ao ler o banco: {e}", "Database read error: {e}")),
        }
        self.data.metrics = metrics::all_metrics(&self.db);
        self.data.generation += 1;
    }

    pub fn save_settings(&mut self) {
        if let Err(e) = self.db.save_settings(&self.settings) {
            self.toast_error(trf!("Não consegui salvar: {e}", "Could not save: {e}"));
        }
        AUTO_UPDATE.store(self.settings.auto_update, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn go(&mut self, page: Page) {
        if self.page != page {
            let prev = std::mem::replace(&mut self.page, page);
            if !matches!(prev, Page::Onboarding) {
                self.history.push(prev);
                if self.history.len() > 50 {
                    self.history.remove(0);
                }
            }
        }
    }

    pub fn back(&mut self) {
        if let Some(p) = self.history.pop() {
            self.page = p;
        } else {
            self.page = Page::Dashboard;
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast {
            text: text.into(),
            error: false,
            until: Instant::now() + Duration::from_secs(5),
        });
    }

    pub fn toast_error(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast {
            text: text.into(),
            error: true,
            until: Instant::now() + Duration::from_secs(10),
        });
    }

    pub fn job_running(&self) -> bool {
        self.job.lock().map(|j| j.running).unwrap_or(false)
    }

    /// Imports the files in /fits that are not in the database yet.
    pub fn import_from_folder(&mut self, quiet: bool) {
        let files = import::scan(&self.paths.fits);
        let pending = import::pending(&self.db, &files);
        if pending.is_empty() {
            if !quiet {
                self.toast(tr!("Nenhum arquivo novo em /fits.", "No new files in /fits."));
            }
            return;
        }
        self.start_job(JobKind::Import {
            files: pending,
            remember_errors: true,
        });
    }

    pub fn import_picked_files(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title(tr!("Importar arquivos FIT", "Import FIT files"))
            .add_filter("FIT", &["fit", "FIT"])
            .pick_files();
        if let Some(files) = picked
            && !files.is_empty()
        {
            self.start_job(JobKind::Import {
                files,
                remember_errors: false,
            });
        }
    }

    /// Runs an import or recalculation on a background thread with its own
    /// database connection; the UI keeps working and shows the progress.
    pub fn start_job(&mut self, kind: JobKind) {
        {
            let mut j = self.job.lock().unwrap();
            if j.running {
                return;
            }
            *j = JobState {
                running: true,
                label: match &kind {
                    JobKind::Import { .. } => tr!("Importando", "Importing").into(),
                    JobKind::Recalculate => tr!("Recalculando métricas", "Recalculating metrics").into(),
                },
                ..Default::default()
            };
        }
        let job = self.job.clone();
        let db_path = self.paths.db.clone();
        let backups = self.paths.backups.clone();
        std::thread::spawn(move || {
            let result = match Db::open(&db_path, Some(&backups)) {
                Err(e) => JobResult::Failed(e.to_string()),
                Ok(mut db) => match kind {
                    JobKind::Import { files, remember_errors } => {
                        let j2 = job.clone();
                        let r = import::import_files(&mut db, &files, remember_errors, &mut |done, total, name| {
                            if let Ok(mut j) = j2.lock() {
                                j.done = done;
                                j.total = total;
                                j.current = name.to_string();
                            }
                        });
                        JobResult::Import(r)
                    }
                    JobKind::Recalculate => {
                        let j2 = job.clone();
                        match metrics::recalculate_all(&db, &mut |done, total| {
                            if let Ok(mut j) = j2.lock() {
                                j.done = done;
                                j.total = total;
                            }
                        }) {
                            Ok(n) => JobResult::Recalculate(n),
                            Err(e) => JobResult::Failed(e.to_string()),
                        }
                    }
                },
            };
            if let Ok(mut j) = job.lock() {
                j.running = false;
                j.finished = Some(result);
            }
        });
    }

    fn poll_job(&mut self, ctx: &egui::Context) {
        let finished = {
            let mut j = self.job.lock().unwrap();
            if j.running {
                ctx.request_repaint_after(Duration::from_millis(150));
            }
            j.finished.take()
        };
        let Some(result) = finished else { return };
        self.reload();
        match result {
            JobResult::Import(r) => {
                let text = trf!("Importação: {}", "Import: {}", r.summary());
                if r.errors.is_empty() {
                    self.toast(text);
                } else {
                    self.toast_error(text);
                    self.show_report = true;
                }
                self.last_report = Some(r);
            }
            JobResult::Recalculate(n) => self.toast(trf!(
                "Métricas recalculadas ({n} atividades).",
                "Metrics recalculated ({n} activities)."
            )),
            JobResult::Failed(e) => self.toast_error(e),
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme != Some(self.settings.theme) {
            ctx.set_theme(match self.settings.theme {
                Theme::Dark => egui::Theme::Dark,
                Theme::Light => egui::Theme::Light,
            });
            self.applied_theme = Some(self.settings.theme);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(RichText::new(APP_NAME).size(22.0).strong().color(widgets::ACCENT));
        });
        ui.add_space(12.0);
        let page = self.page.clone();
        let nav = |ui: &mut egui::Ui, icon: &str, label: &str, selected: bool| -> bool {
            let text = RichText::new(format!("{icon}  {label}")).size(15.0);
            ui.add_sized([ui.available_width(), 30.0], egui::Button::selectable(selected, text))
                .clicked()
        };
        let in_activities = matches!(
            page,
            Page::Activities(_) | Page::Activity(_) | Page::Manual(_) | Page::Compare
        );
        if nav(ui, "📊", tr!("Dashboard", "Dashboard"), page == Page::Dashboard) {
            self.go(Page::Dashboard);
        }
        if nav(ui, "🏃", tr!("Atividades", "Activities"), in_activities) {
            self.go(Page::Activities(None));
        }
        if in_activities {
            let sub: [(Option<Modality>, &str); 5] = [
                (None, tr!("Todas", "All")),
                (Some(Modality::RoadRun), Modality::RoadRun.label()),
                (Some(Modality::Treadmill), Modality::Treadmill.label()),
                (Some(Modality::Tennis), Modality::Tennis.label()),
                (Some(Modality::Strength), Modality::Strength.label()),
            ];
            for (m, label) in sub {
                ui.horizontal(|ui| {
                    ui.add_space(22.0);
                    let sel = page == Page::Activities(m);
                    if ui
                        .add_sized([ui.available_width(), 24.0], egui::Button::selectable(sel, label))
                        .clicked()
                    {
                        self.go(Page::Activities(m));
                    }
                });
            }
        }
        let items: [(&str, &str, Page); 8] = [
            ("📅", tr!("Calendário", "Calendar"), Page::Calendar),
            ("📋", tr!("Planos", "Plans"), Page::Plans),
            ("🎯", tr!("Metas", "Goals"), Page::Goals),
            ("📈", tr!("Evolução", "Progress"), Page::Evolution),
            ("🏆", tr!("Records", "Records"), Page::Records),
            ("🔬", tr!("Análises", "Analysis"), Page::Analysis),
            ("💡", tr!("Insights", "Insights"), Page::Insights),
            ("⚙", tr!("Configurações", "Settings"), Page::Settings),
        ];
        for (icon, label, p) in items {
            if nav(ui, icon, label, page == p) {
                self.go(p);
            }
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
            self.update_notice(ui);
            self.job_status(ui);
            ui.add_space(4.0);
            if ui
                .add_sized(
                    [ui.available_width(), 28.0],
                    egui::Button::new(tr!("➕ Atividade manual", "➕ Manual activity")),
                )
                .clicked()
            {
                self.pages.manual = manual::State::default();
                self.go(Page::Manual(None));
            }
            if ui
                .add_sized(
                    [ui.available_width(), 28.0],
                    egui::Button::new(tr!("📂 Importar arquivos FIT", "📂 Import FIT files")),
                )
                .clicked()
            {
                self.import_picked_files();
            }
        });
    }

    fn job_status(&mut self, ui: &mut egui::Ui) {
        let (running, label, done, total, current) = {
            let j = self.job.lock().unwrap();
            (j.running, j.label.clone(), j.done, j.total, j.current.clone())
        };
        if running {
            let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
            ui.label(RichText::new(current).small().color(ui.visuals().weak_text_color()));
            ui.add(egui::ProgressBar::new(frac).text(format!("{label} {done}/{total}")));
        } else if let Some(r) = &self.last_report
            && !r.errors.is_empty()
            && ui
                .link(
                    RichText::new(trf!(
                        "⚠ {} arquivo(s) com erro",
                        "⚠ {} file(s) with errors",
                        r.errors.len()
                    ))
                    .color(widgets::WARN),
                )
                .clicked()
        {
            self.show_report = true;
        }
    }

    fn update_notice(&mut self, ui: &mut egui::Ui) {
        use crate::update::Estado;
        match crate::update::estado() {
            Estado::Disponivel(v) => {
                ui.horizontal_wrapped(|ui| {
                    if v.instalavel()
                        && ui
                            .link(
                                RichText::new(trf!("⬆ Atualizar para v{}", "⬆ Update to v{}", v.numero))
                                    .color(widgets::ACCENT),
                            )
                            .on_hover_text(tr!(
                                "Baixa a versão nova do GitHub, confere o SHA-256 e reabre o app.",
                                "Downloads the new version from GitHub, checks its SHA-256 and reopens the app."
                            ))
                            .clicked()
                    {
                        crate::update::instalar(v.clone());
                    }
                    ui.hyperlink_to(tr!("novidades", "what's new"), &v.pagina);
                });
            }
            Estado::Baixando(_, f) => {
                ui.add(egui::ProgressBar::new(f).text(tr!("Baixando atualização", "Downloading update")));
            }
            Estado::FalhaInstalacao(_, e) => {
                ui.label(RichText::new(e).small().color(widgets::BAD));
            }
            Estado::Reiniciando(_) => {
                ui.label(tr!("Reiniciando…", "Restarting…"));
            }
            _ => {}
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -16.0])
            .show(ctx, |ui| {
                for t in &self.toasts {
                    let fill = if t.error {
                        widgets::BAD.gamma_multiply(0.85)
                    } else {
                        widgets::ACCENT.gamma_multiply(0.85)
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .corner_radius(8.0)
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.set_max_width(380.0);
                            ui.label(RichText::new(&t.text).color(Color32::WHITE));
                        });
                    ui.add_space(6.0);
                }
            });
    }

    fn report_window(&mut self, ctx: &egui::Context) {
        if !self.show_report {
            return;
        }
        let mut open = true;
        egui::Window::new(tr!("Resultado da importação", "Import result"))
            .open(&mut open)
            .collapsible(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                let Some(r) = &self.last_report else { return };
                ui.label(r.summary());
                if !r.same_activity.is_empty() {
                    ui.add_space(6.0);
                    ui.label(tr!(
                        "Arquivos diferentes do mesmo treino (não duplicados):",
                        "Different files of the same workout (not duplicated):"
                    ));
                    for n in &r.same_activity {
                        ui.label(format!("  • {n}"));
                    }
                }
                if !r.errors.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(tr!("Arquivos com erro:", "Files with errors:")).strong());
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        for (n, e) in &r.errors {
                            ui.label(RichText::new(format!("• {n}: {e}")).color(widgets::BAD));
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(tr!(
                            "Os demais arquivos foram importados normalmente. Os arquivos originais não foram alterados.",
                            "The other files were imported normally. The original files were not changed."
                        ))
                        .small(),
                    );
                }
            });
        if !open {
            self.show_report = false;
        }
    }
}

impl App {
    /// Pages captured by FITLEDGER_SCREENSHOTS.
    fn screenshot_plan(&self) -> Vec<(String, Page, Option<activity::Tab>)> {
        let mut v = vec![
            ("dashboard".to_string(), Page::Dashboard, None),
            ("activities".into(), Page::Activities(None), None),
        ];
        let run = self
            .data
            .activities
            .iter()
            .find(|a| a.modality == Modality::RoadRun && a.has_gps);
        if let Some(a) = run {
            for (name, tab) in [
                ("activity-charts", activity::Tab::Charts),
                ("activity-splits", activity::Tab::Splits),
                ("activity-zones", activity::Tab::Zones),
                ("activity-drift", activity::Tab::Drift),
                ("activity-route", activity::Tab::Map),
                ("activity-notes", activity::Tab::Notes),
            ] {
                v.push((name.into(), Page::Activity(a.id), Some(tab)));
            }
        }
        if let Some(t) = self.data.activities.iter().find(|a| a.modality == Modality::Tennis) {
            v.push((
                "activity-tennis".into(),
                Page::Activity(t.id),
                Some(activity::Tab::Charts),
            ));
        }
        v.extend([
            ("tennis".into(), Page::Activities(Some(Modality::Tennis)), None),
            ("strength".into(), Page::Activities(Some(Modality::Strength)), None),
            ("calendar".into(), Page::Calendar, None),
            ("plans".into(), Page::Plans, None),
            ("goals".into(), Page::Goals, None),
            ("evolution".into(), Page::Evolution, None),
            ("records".into(), Page::Records, None),
            ("analysis".into(), Page::Analysis, None),
            ("insights".into(), Page::Insights, None),
            ("settings".into(), Page::Settings, None),
            ("manual".into(), Page::Manual(None), None),
        ]);
        if let Ok(only) = std::env::var("FITLEDGER_SCREENSHOT_ONLY") {
            let keep: Vec<&str> = only.split(',').map(str::trim).collect();
            v.retain(|(n, _, _)| keep.contains(&n.as_str()));
        }
        v
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(sh) = self.shooter.as_mut() else { return };
        // a saved screenshot arrives as an event
        let mut saved = false;
        ctx.input(|i| {
            for e in &i.events {
                if let egui::Event::Screenshot { image, .. } = e {
                    if let Some((name, _, _)) = self.shots.get(sh.index) {
                        let path = sh.dir.join(format!("{name}.png"));
                        let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                        let _ = crate::screenshots::write_png(&path, image.size[0] as u32, image.size[1] as u32, &rgba);
                    }
                    saved = true;
                }
            }
        });
        if saved {
            sh.index += 1;
            sh.frames = 0;
            sh.requested = false;
        }
        let Some((_, page, tab)) = self.shots.get(sh.index).cloned() else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        if sh.frames == 0 {
            self.page = page;
            if let Some(t) = tab {
                self.pages.activity.tab = t;
            }
        }
        if let Some(t) = tab {
            self.pages.activity.tab = t;
        }
        let sh = self.shooter.as_mut().unwrap();
        sh.frames += 1;
        if sh.frames == 25 && !sh.requested {
            sh.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ctx.request_repaint();
    }
}

static AUTO_UPDATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

fn auto_update_enabled() -> bool {
    AUTO_UPDATE.load(std::sync::atomic::Ordering::Relaxed)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_theme(&ctx);
        self.poll_job(&ctx);
        self.screenshot_step(&ctx);

        if self.page != Page::Onboarding {
            egui::Panel::left("nav")
                .resizable(false)
                .exact_size(210.0)
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(8.0))
                .show(ui, |ui| self.sidebar(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::symmetric(20, 14)))
            .show(ui, |ui| match self.page.clone() {
                Page::Onboarding => onboarding::show(self, ui),
                Page::Dashboard => dashboard::show(self, ui),
                Page::Activities(m) => activities::show(self, ui, m),
                Page::Activity(id) => activity::show(self, ui, id),
                Page::Manual(id) => manual::show(self, ui, id),
                Page::Compare => compare::show(self, ui),
                Page::Calendar => calendar::show(self, ui),
                Page::Plans => plans::show(self, ui),
                Page::Goals => goals::show(self, ui),
                Page::Evolution => evolution::show(self, ui),
                Page::Records => records::show(self, ui),
                Page::Analysis => analysis::show(self, ui),
                Page::Insights => insights_page::show(self, ui),
                Page::Settings => settings_page::show(self, ui),
            });
        self.report_window(&ctx);
        self.toasts_ui(&ctx);
    }
}
