//! FIT import: scans the /fits folder (or files picked by the user), skips
//! what is already in the database and imports the rest. One bad file never
//! stops the others. Original files are only read, never changed.

use crate::analysis::metrics;
use crate::db::Db;
use crate::fit;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Larger files are certainly not activities from a watch.
const MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Imported(i64),
    /// Same file already imported.
    AlreadyImported,
    /// Different file, same workout (re-export): id of the existing activity.
    SameActivity(i64),
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub imported: Vec<(String, i64)>,
    pub skipped: usize,
    pub same_activity: Vec<String>,
    pub errors: Vec<(String, String)>,
}

impl Report {
    pub fn summary(&self) -> String {
        let mut parts = vec![trf!("{} nova(s)", "{} new", self.imported.len())];
        if self.skipped + self.same_activity.len() > 0 {
            parts.push(trf!(
                "{} já existente(s)",
                "{} already there",
                self.skipped + self.same_activity.len()
            ));
        }
        if !self.errors.is_empty() {
            parts.push(trf!("{} com erro", "{} with errors", self.errors.len()));
        }
        parts.join(", ")
    }
}

/// Errors remembered between runs (hash → (file name, message)), so a broken
/// file in /fits is reported once and not retried on every start.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct KnownErrors(pub BTreeMap<String, (String, String)>);

impl KnownErrors {
    const KEY: &'static str = "import_errors";

    pub fn load(db: &Db) -> Self {
        db.get_kv(Self::KEY)
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, db: &Db) {
        if let Ok(s) = serde_json::to_string(self) {
            let _ = db.set_kv(Self::KEY, &s);
        }
    }
}

/// All .fit files in the folder and its subfolders (case-insensitive extension).
pub fn scan(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fit")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn display_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn read_file(p: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > MAX_FILE_SIZE {
        return Err(tr!(
            "arquivo grande demais para ser uma atividade",
            "file too large to be an activity"
        )
        .into());
    }
    std::fs::read(p).map_err(|e| e.to_string())
}

/// Files in `paths` that are not in the database yet (and did not fail before).
pub fn pending(db: &Db, paths: &[PathBuf]) -> Vec<PathBuf> {
    let known = db.known_hashes().unwrap_or_default();
    let failed = KnownErrors::load(db);
    paths
        .iter()
        .filter(|p| {
            read_file(p)
                .ok()
                .and_then(|b| crate::hash::sha256_hex(&b))
                .is_some_and(|h| !known.contains(&h) && !failed.0.contains_key(&h))
        })
        .cloned()
        .collect()
}

/// Imports one file already in memory.
pub fn import_bytes(db: &mut Db, name: &str, bytes: &[u8]) -> Result<Outcome, String> {
    let hash = crate::hash::sha256_hex(bytes).ok_or("SHA-256")?;
    if db.fit_file_exists(&hash).map_err(|e| e.to_string())? {
        return Ok(Outcome::AlreadyImported);
    }
    let messages = fit::decode::decode(bytes).map_err(|e| e.to_string())?;
    let parsed = fit::activity::parse(&messages).map_err(|e| e.to_string())?;
    if let Some(existing) = db.find_same_activity(&parsed).map_err(|e| e.to_string())? {
        return Ok(Outcome::SameActivity(existing));
    }
    let id = db
        .insert_fit_activity(&hash, name, bytes, &parsed, None)
        .map_err(|e| e.to_string())?;
    let settings = db.load_settings();
    let observed = db.observed_max_hr();
    if let Ok(Some(a)) = db.activity(id) {
        metrics::update_activity(db, &a, &settings, observed).map_err(|e| e.to_string())?;
    }
    Ok(Outcome::Imported(id))
}

/// Imports a list of files; `progress(done, total, current name)`.
/// `remember_errors`: keep failures so they are not retried automatically.
pub fn import_files(
    db: &mut Db,
    paths: &[PathBuf],
    remember_errors: bool,
    progress: &mut dyn FnMut(usize, usize, &str),
) -> Report {
    let mut report = Report::default();
    let mut known_errors = KnownErrors::load(db);
    for (i, p) in paths.iter().enumerate() {
        let name = display_name(p);
        progress(i, paths.len(), &name);
        let result = read_file(p).and_then(|bytes| {
            let r = import_bytes(db, &name, &bytes);
            if let (Err(e), true) = (&r, remember_errors)
                && let Some(h) = crate::hash::sha256_hex(&bytes)
            {
                known_errors.0.insert(h, (name.clone(), e.clone()));
            }
            r
        });
        match result {
            Ok(Outcome::Imported(id)) => report.imported.push((name, id)),
            Ok(Outcome::AlreadyImported) => report.skipped += 1,
            Ok(Outcome::SameActivity(_)) => report.same_activity.push(name),
            Err(e) => report.errors.push((name, e)),
        }
    }
    progress(paths.len(), paths.len(), "");
    if remember_errors {
        known_errors.save(db);
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::writer::Synthetic;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fitledger-import-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn imports_new_files_once_and_survives_bad_ones() {
        let dir = temp_dir("a");
        let run1 = Synthetic::run(1_100_000_000, 1800, 2.78, 150.0, 165.0, true).encode();
        let run2 = Synthetic::run(1_100_100_000, 1200, 3.0, 150.0, 160.0, false).encode();
        std::fs::write(dir.join("a.fit"), &run1).unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("B.FIT"), &run2).unwrap();
        std::fs::write(dir.join("broken.fit"), &run1[..run1.len() / 3]).unwrap();
        std::fs::write(dir.join("notes.txt"), b"not a fit").unwrap();
        let before = std::fs::read(dir.join("a.fit")).unwrap();

        let mut db = Db::open_in_memory().unwrap();
        let files = scan(&dir);
        assert_eq!(files.len(), 3);
        assert_eq!(pending(&db, &files).len(), 3);

        let r = import_files(&mut db, &files, true, &mut |_, _, _| {});
        assert_eq!(r.imported.len(), 2, "{r:?}");
        assert_eq!(r.errors.len(), 1);
        assert!(
            r.errors[0].1.contains("trunc") || r.errors[0].1.contains("incomplet"),
            "{:?}",
            r.errors
        );

        // nothing pending anymore: broken file is remembered, good ones imported
        assert!(pending(&db, &files).is_empty());
        // running again imports nothing
        let r = import_files(&mut db, &files, true, &mut |_, _, _| {});
        assert!(r.imported.is_empty());
        assert_eq!(db.activities().unwrap().len(), 2);

        // the same workout renamed is not duplicated
        let r = import_files(&mut db, &[dir.join("a.fit")], false, &mut |_, _, _| {});
        assert_eq!(r.skipped, 1);

        // original file untouched
        assert_eq!(std::fs::read(dir.join("a.fit")).unwrap(), before);

        let acts = db.activities().unwrap();
        let road = acts.iter().find(|a| a.has_gps).unwrap();
        assert!(road.moving_s.unwrap() > 1700.0);
        assert!(road.load.unwrap() > 10.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn re_exported_workout_is_detected() {
        let mut db = Db::open_in_memory().unwrap();
        let a = Synthetic::run(1_100_000_000, 900, 2.78, 150.0, 165.0, true);
        assert!(matches!(
            import_bytes(&mut db, "x.fit", &a.encode()).unwrap(),
            Outcome::Imported(_)
        ));
        // same device + creation time but different bytes (e.g. other export tool)
        let mut b = Synthetic::run(1_100_000_000, 900, 2.78, 150.0, 166.0, true);
        b.serial = a.serial;
        assert!(matches!(
            import_bytes(&mut db, "y.fit", &b.encode()).unwrap(),
            Outcome::SameActivity(_)
        ));
        assert_eq!(db.activities().unwrap().len(), 1);
    }

    /// Real files: `FITLEDGER_REAL_FITS=<folder> cargo test -- --ignored real_files`.
    /// Never commit real files: they contain GPS tracks.
    #[test]
    #[ignore]
    fn real_files() {
        let dir = std::env::var("FITLEDGER_REAL_FITS").expect("set FITLEDGER_REAL_FITS");
        let files = scan(Path::new(&dir));
        let mut db = Db::open_in_memory().unwrap();
        let r = import_files(&mut db, &files, false, &mut |_, _, _| {});
        println!("{}", r.summary());
        for (name, e) in &r.errors {
            println!("ERROR {name}: {e}");
        }
        for a in db.activities().unwrap() {
            let m = metrics::load_metrics(&db, a.id).unwrap();
            println!(
                "{} {:?} dist={:?} elapsed={:?} moving={:?} hr={:?}/{:?} cad={:?} pace={:?} load={:.0} drift={:?} quality={:?} pace_hr_pts={} km_hr={:?}",
                a.local_start(),
                a.modality,
                a.distance(),
                a.elapsed_s,
                a.moving_s,
                a.avg_hr,
                a.max_hr,
                a.avg_cadence,
                a.pace_s_per_km().map(crate::format::pace),
                m.load,
                m.drift
                    .as_ref()
                    .map(|d| (d.hr1.round(), d.hr2.round(), d.decoupling_pct)),
                m.quality,
                m.pace_hr.len(),
                m.km_hr
            );
        }
        assert!(r.errors.is_empty());
    }
}
