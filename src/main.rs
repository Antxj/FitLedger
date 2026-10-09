//! FitLedger - local sports diary: FIT import, running analysis, training
//! plans, goals, strength training and tennis.
#![cfg_attr(not(test), windows_subsystem = "windows")]

// first: the tr!/trf! macros must be defined before the modules
#[macro_use]
mod i18n;
mod analysis;
mod backup;
mod db;
mod demo;
mod export;
mod fit;
mod format;
mod hash;
mod import;
mod model;
mod paths;
mod planning;
mod screenshots;
mod settings;
mod single;
mod ui;
mod update;

use eframe::egui;
use std::time::Duration;

fn main() {
    // started by the updater: wait for the old version to close first
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == update::ARG_APOS_ATUALIZAR)
        && let Some(pid) = args.get(i + 1).and_then(|p| p.parse().ok())
    {
        update::aguardar_processo(pid, Duration::from_secs(20));
    }
    update::limpar_restos();

    if !single::acquire_or_signal() {
        return;
    }

    let paths = paths::Paths::resolve();
    if let Err(msg) = paths.prepare() {
        fatal(&msg);
        return;
    }
    let db = match db::Db::open(&paths.db, Some(&paths.backups)) {
        Ok(db) => db,
        Err(e) => {
            fatal(&trf!(
                "Não consegui abrir o banco de dados ({}):\n{e}",
                "Could not open the database ({}):\n{e}",
                paths.db.display()
            ));
            return;
        }
    };
    // language before the first window text
    let lang = db.load_settings().language;
    i18n::aplicar(lang.as_deref().and_then(i18n::Idioma::do_codigo));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("FitLedger v{}", env!("CARGO_PKG_VERSION")))
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(icon()),
        ..Default::default()
    };
    let result = eframe::run_native(
        "FitLedger",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, db, paths)))),
    );
    if let Err(e) = result {
        fatal(&trf!(
            "Não consegui abrir a janela (OpenGL): {e}",
            "Could not open the window (OpenGL): {e}"
        ));
    }
}

/// Window icon drawn in code (the .exe icon comes from app.rc).
fn icon() -> egui::IconData {
    let size = 64u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 - c, y as f32 - c);
            let r = (dx * dx + dy * dy).sqrt();
            let i = ((y * size + x) * 4) as usize;
            if r <= c {
                // blue disc with a white rising line (a chart)
                let (mut cr, mut cg, mut cb) = (0x3bu8, 0x82u8, 0xf6u8);
                let fx = x as f32 / size as f32;
                let line_y = size as f32 * (0.72 - 0.45 * fx + 0.08 * (fx * 9.0).sin());
                if (y as f32 - line_y).abs() < 3.0 && (0.18..0.84).contains(&fx) {
                    (cr, cg, cb) = (255, 255, 255);
                }
                rgba[i..i + 4].copy_from_slice(&[cr, cg, cb, 255]);
            }
        }
    }
    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

/// Error before the window exists: a Windows message box.
fn fatal(msg: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let text = single::wide(msg);
    let caption = single::wide("FitLedger");
    // SAFETY: both strings are NUL-terminated UTF-16 buffers alive during the call;
    // a null owner window is allowed. 0x10 = MB_ICONERROR.
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10) };
}
