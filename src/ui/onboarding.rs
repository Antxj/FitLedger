//! First run (item 34): optional profile, then "We found N activities" → Import.

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
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let mut start: Option<usize> = None;
    let s = &mut app.pages.onboarding;
    ui.vertical_centered(|ui| {
        ui.add_space(30.0);
        ui.label(RichText::new("FitLedger").size(34.0).strong().color(widgets::ACCENT));
        ui.label(weak(ui, tr!("Seu diário esportivo local", "Your local sports diary")));
        ui.add_space(20.0);
    });
    let width = 520.0_f32.min(ui.available_width());
    ui.vertical_centered(|ui| {
        ui.set_max_width(width);
        widgets::card_frame(ui).show(ui, |ui| {
            ui.set_width(width - 28.0);
            if s.step == 0 {
                ui.label(RichText::new(tr!("Bem-vindo! Vamos configurar o básico.", "Welcome! Let's set up the basics.")).size(17.0).strong());
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
                ui.label(weak(ui, tr!(
                    "Tudo é opcional. Esses dados servem para calcular as zonas de frequência cardíaca e a carga dos treinos, e ficam só neste computador.",
                    "Everything is optional. This data is used to compute heart-rate zones and training load, and it stays on this computer only."
                )));
                ui.add_space(8.0);
                egui::Grid::new("onb").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    let field = |ui: &mut Ui, label: &str, buf: &mut String, hint: &str| {
                        ui.label(label);
                        ui.add(egui::TextEdit::singleline(buf).hint_text(hint).desired_width(120.0));
                        ui.end_row();
                    };
                    field(ui, tr!("Idade", "Age"), &mut s.age, tr!("anos", "years"));
                    field(ui, tr!("Peso", "Weight"), &mut s.weight, "kg");
                    field(ui, tr!("Altura", "Height"), &mut s.height, "cm");
                    field(ui, tr!("FC máxima", "Max HR"), &mut s.hr_max, tr!("bpm (se souber)", "bpm (if known)"));
                    field(ui, tr!("FC de repouso", "Resting HR"), &mut s.hr_rest, "bpm");
                });
                ui.add_space(10.0);
                if ui.button(RichText::new(tr!("Continuar →", "Continue →")).strong()).clicked() {
                    let st = &mut app.settings;
                    st.age = format::parse_num(&s.age).filter(|v| (5.0..120.0).contains(v)).map(|v| v as u32);
                    st.weight_kg = format::parse_num(&s.weight).filter(|v| *v > 20.0);
                    st.height_cm = format::parse_num(&s.height).filter(|v| *v > 80.0);
                    st.hr_max = format::parse_num(&s.hr_max).filter(|v| (120.0..240.0).contains(v));
                    st.hr_rest = format::parse_num(&s.hr_rest).filter(|v| (25.0..120.0).contains(v));
                    s.step = 1;
                    s.found = None;
                }
            } else {
                if s.found.is_none() {
                    let files = crate::import::scan(&app.paths.fits);
                    s.found = Some(crate::import::pending(&app.db, &files).len());
                }
                let n = s.found.unwrap_or(0);
                ui.label(RichText::new(tr!("Arquivos FIT", "FIT files")).size(17.0).strong());
                ui.label(weak(ui, trf!(
                    "Pasta monitorada: {}",
                    "Watched folder: {}",
                    app.paths.fits.display()
                )));
                ui.add_space(8.0);
                if n > 0 {
                    ui.label(RichText::new(trf!("Encontramos {n} atividades.", "We found {n} activities.")).size(18.0));
                } else {
                    ui.label(tr!(
                        "Nenhum arquivo .fit encontrado ainda. Copie seus arquivos para essa pasta; eles serão importados sempre que o FitLedger abrir.",
                        "No .fit files found yet. Copy your files to that folder; they will be imported every time FitLedger opens."
                    ));
                }
                ui.add_space(4.0);
                widgets::note(
                    ui,
                    tr!(
                        "Os dados ficam na pasta \"data\" ao lado do FitLedger.exe. Se apagar a pasta do aplicativo, o histórico também é apagado: use Configurações → Criar backup de vez em quando.",
                        "Data is kept in the \"data\" folder next to FitLedger.exe. Deleting the app folder also deletes the history: use Settings → Create backup now and then."
                    ),
                    widgets::WARN,
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button(tr!("← Voltar", "← Back")).clicked() {
                        s.step = 0;
                    }
                    let label = if n > 0 { tr!("Importar", "Import") } else { tr!("Começar", "Start") };
                    if ui.button(RichText::new(label).strong()).clicked() {
                        start = Some(n);
                    }
                    if ui.button(tr!("🔄 Procurar de novo", "🔄 Look again")).clicked() {
                        s.found = None;
                    }
                });
            }
        });
    });
    if let Some(n) = start {
        app.settings.onboarded = true;
        app.save_settings();
        if n > 0 {
            app.import_from_folder(true);
        }
        app.go(Page::Dashboard);
    }
}
