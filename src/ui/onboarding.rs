//! First run: profile (for heart-rate zones), runs found in /fits, first goal.

use super::goal_page::{self, Form};
use super::widgets::{self, weak};
use super::{App, Page};
use crate::format;
use crate::i18n::Idioma;
use eframe::egui::{self, RichText, Ui};

#[derive(Default)]
pub struct State {
    step: usize,
    age: String,
    weight: String,
    height: String,
    hr_max: String,
    hr_rest: String,
    found: Option<usize>,
    form: Option<Form>,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| content(app, ui));
}

fn content(app: &mut App, ui: &mut Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        ui.label(RichText::new("FitLedger").size(34.0).strong().color(widgets::ACCENT));
        ui.label(weak(
            ui,
            tr!(
                "Seu treinador de corrida, no seu computador",
                "Your running coach, on your computer"
            ),
        ));
        ui.add_space(16.0);
    });
    let width = 640.0_f32.min(ui.available_width());
    let step = app.pages.onboarding.step;
    let mut next: Option<usize> = None;
    let mut finish = false;
    ui.vertical_centered(|ui| {
        ui.set_max_width(width);
        ui.label(weak(ui, trf!("Passo {} de 3", "Step {} of 3", step + 1)));
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(width - 28.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| match step {
                0 => {
                    if profile(app, ui) {
                        next = Some(1);
                    }
                }
                1 => match runs(app, ui) {
                    Some(true) => next = Some(2),
                    Some(false) => next = Some(0),
                    None => {}
                },
                _ => {
                    let s = &mut app.pages.onboarding;
                    if s.form.is_none() {
                        s.form = Some(Form::new(Default::default()));
                    }
                    ui.label(RichText::new(tr!("Qual é a sua meta?", "What is your goal?")).size(17.0).strong());
                    ui.label(weak(
                        ui,
                        tr!(
                            "Exemplos: \"correr 5 km\", \"5 km em 30 min\", \"5 km a 6:30/km com FC média até 160 bpm\". Dá para mudar quando quiser.",
                            "Examples: \"run 5 km\", \"5 km in 30 min\", \"5 km at 6:30/km with avg HR up to 160 bpm\". You can change it anytime."
                        ),
                    ));
                    ui.add_space(6.0);
                    if let Some(mut g) = goal_page::form(ui, s.form.as_mut().unwrap(), tr!("Criar meu plano →", "Create my plan →")) {
                        g.start_date = chrono::Local::now().date_naive();
                        if g.target_hr.is_some() && g.target_time_s.is_none() && g.hr_pace.is_none() {
                            g.hr_pace = app
                                .coach
                                .as_ref()
                                .map(|c| (c.fitness.easy_pace / 5.0).round() * 5.0)
                                .or(Some(crate::coach::fitness::BEGINNER_EASY_PACE));
                        }
                        match app.db.set_goal(&g) {
                            Ok(_) => finish = true,
                            Err(e) => app.toast_error(e.to_string()),
                        }
                    }
                    if ui.link(tr!("Pular por enquanto", "Skip for now")).clicked() {
                        finish = true;
                    }
                }
            });
        });
    });
    if let Some(n) = next {
        app.pages.onboarding.step = n;
    }
    if finish {
        app.settings.onboarded = true;
        app.save_settings();
        app.refresh_coach(true);
        app.go(Page::Today);
    }
}

/// Language and profile. Returns true on "continue".
fn profile(app: &mut App, ui: &mut Ui) -> bool {
    ui.label(
        RichText::new(tr!("Bem-vindo! Primeiro, o básico.", "Welcome! First, the basics."))
            .size(17.0)
            .strong(),
    );
    ui.horizontal(|ui| {
        ui.label(tr!("Idioma:", "Language:"));
        for i in [Idioma::Portugues, Idioma::Ingles] {
            let sel = crate::i18n::pt() == (i == Idioma::Portugues);
            if ui.selectable_label(sel, i.nome()).clicked() {
                app.settings.language = Some(i.codigo().into());
                crate::i18n::aplicar(Some(i));
            }
        }
    });
    ui.label(weak(
        ui,
        tr!(
            "A idade e a FC servem para calcular suas zonas de frequência cardíaca, que guiam o ritmo de cada treino. Tudo fica só neste computador.",
            "Age and HR are used to compute your heart-rate zones, which set the effort of every session. Everything stays on this computer."
        ),
    ));
    ui.add_space(8.0);
    let s = &mut app.pages.onboarding;
    egui::Grid::new("onb")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            // the unit stays visible next to the field, also after typing
            let field = |ui: &mut Ui, label: &str, buf: &mut String, unit: &str, note: &str| {
                ui.label(label);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(buf).desired_width(80.0));
                    ui.label(unit);
                    if !note.is_empty() {
                        ui.label(weak(ui, note).small());
                    }
                });
                ui.end_row();
            };
            field(ui, tr!("Idade", "Age"), &mut s.age, tr!("anos", "years"), "");
            field(
                ui,
                tr!("FC máxima", "Max HR"),
                &mut s.hr_max,
                "bpm",
                tr!("se souber (senão é estimada)", "if known (otherwise estimated)"),
            );
            field(
                ui,
                tr!("FC de repouso", "Resting HR"),
                &mut s.hr_rest,
                "bpm",
                tr!("ao acordar", "on waking up"),
            );
            field(
                ui,
                tr!("Peso", "Weight"),
                &mut s.weight,
                "kg",
                tr!("opcional", "optional"),
            );
            field(
                ui,
                tr!("Altura", "Height"),
                &mut s.height,
                "cm",
                tr!("opcional", "optional"),
            );
        });
    ui.add_space(10.0);
    if ui
        .button(RichText::new(tr!("Continuar →", "Continue →")).strong())
        .clicked()
    {
        let st = &mut app.settings;
        st.age = format::parse_num(&s.age)
            .filter(|v| (5.0..120.0).contains(v))
            .map(|v| v as u32);
        st.weight_kg = format::parse_num(&s.weight).filter(|v| *v > 20.0);
        st.height_cm = format::parse_num(&s.height).filter(|v| *v > 80.0);
        st.hr_max = format::parse_num(&s.hr_max).filter(|v| (120.0..240.0).contains(v));
        st.hr_rest = format::parse_num(&s.hr_rest).filter(|v| (25.0..120.0).contains(v));
        s.found = None;
        app.save_settings();
        return true;
    }
    false
}

/// Runs in /fits. Some(true) = continue, Some(false) = back.
fn runs(app: &mut App, ui: &mut Ui) -> Option<bool> {
    if app.pages.onboarding.found.is_none() {
        let files = crate::import::scan(&app.paths.fits);
        app.pages.onboarding.found = Some(crate::import::pending(&app.db, &files).len());
    }
    let n = app.pages.onboarding.found.unwrap_or(0);
    ui.label(RichText::new(tr!("Suas corridas", "Your runs")).size(17.0).strong());
    ui.label(weak(
        ui,
        trf!("Pasta monitorada: {}", "Watched folder: {}", app.paths.fits.display()),
    ));
    ui.add_space(6.0);
    let busy = app.job_running();
    if n > 0 {
        ui.label(RichText::new(trf!("Encontramos {n} arquivo(s) .fit.", "We found {n} .fit file(s).")).size(18.0));
        if ui
            .add_enabled(
                !busy,
                egui::Button::new(RichText::new(tr!("Importar", "Import")).strong()),
            )
            .clicked()
        {
            app.import_from_folder(true);
            app.pages.onboarding.found = Some(0);
        }
    } else if !app.data.activities.is_empty() {
        ui.label(
            RichText::new(trf!(
                "{} corridas importadas.",
                "{} runs imported.",
                app.data.activities.len()
            ))
            .size(18.0)
            .color(widgets::GOOD),
        );
    } else {
        ui.label(tr!(
            "Nenhum arquivo .fit novo. Exporte as corridas no app do relógio (por exemplo, o Zepp exporta cada treino em .fit) e copie os arquivos para essa pasta. Sem corridas, o plano começa leve e se ajusta depois.",
            "No new .fit files. Export your runs from the watch app (Zepp, for example, exports each workout as .fit) and copy the files to that folder. Without runs, the plan starts easy and adjusts later."
        ));
    }
    if busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(tr!("Importando…", "Importing…"));
        });
    }
    ui.add_space(4.0);
    widgets::note(
        ui,
        tr!(
            "Seus dados ficam na pasta \"data\" ao lado do FitLedger.exe. Apagar a pasta do aplicativo apaga o histórico: use Configurações → Criar backup de vez em quando.",
            "Your data is kept in the \"data\" folder next to FitLedger.exe. Deleting the app folder deletes the history: use Settings → Create backup now and then."
        ),
        widgets::WARN,
    );
    ui.add_space(8.0);
    let mut r = None;
    ui.horizontal(|ui| {
        if ui.button(tr!("← Voltar", "← Back")).clicked() {
            r = Some(false);
        }
        if ui
            .add_enabled(
                !busy,
                egui::Button::new(RichText::new(tr!("Continuar →", "Continue →")).strong()),
            )
            .clicked()
        {
            r = Some(true);
        }
        if ui.button(tr!("🔄 Procurar de novo", "🔄 Look again")).clicked() {
            app.pages.onboarding.found = None;
        }
    });
    r
}
