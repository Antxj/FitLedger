//! Backup and restore (item 31). A backup is a consistent copy of the whole
//! SQLite database (`VACUUM INTO`): activities, original FIT files, plans,
//! goals, strength data, notes and settings, all in one `.fitledger` file.

use crate::db::{Db, schema};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

pub const EXTENSION: &str = "fitledger";

pub fn default_file_name() -> String {
    format!(
        "FitLedger-backup-{}.{EXTENSION}",
        chrono::Local::now().format("%Y-%m-%d-%H%M")
    )
}

pub fn create(db: &Db, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        std::fs::remove_file(dest).map_err(|e| e.to_string())?;
    }
    db.conn
        .execute("VACUUM INTO ?1", [dest.to_string_lossy()])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// What a backup file contains (shown before restoring).
#[derive(Debug, Clone, PartialEq)]
pub struct BackupInfo {
    pub activities: i64,
    pub fit_files: i64,
    pub schema: i64,
}

/// Opens the file read-only and checks that it is a valid FitLedger database.
pub fn inspect(src: &Path) -> Result<BackupInfo, String> {
    let invalid = || {
        tr!(
            "o arquivo não é um backup do FitLedger",
            "the file is not a FitLedger backup"
        )
        .to_string()
    };
    let conn = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|_| invalid())?;
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|_| invalid())?;
    if check != "ok" {
        return Err(tr!("o backup está corrompido", "the backup is corrupted").into());
    }
    let schema_v: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|_| invalid())?;
    if schema_v < 1 {
        return Err(invalid());
    }
    if schema_v > schema::latest_version() {
        return Err(tr!(
            "o backup foi feito por uma versão mais nova do FitLedger; atualize o aplicativo",
            "the backup was made by a newer FitLedger version; update the app"
        )
        .into());
    }
    let count = |t: &str| conn.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get::<_, i64>(0));
    Ok(BackupInfo {
        activities: count("activity").map_err(|_| invalid())?,
        fit_files: count("fit_file").map_err(|_| invalid())?,
        schema: schema_v,
    })
}

/// Replaces the database at `db_path` with the backup. The current database
/// is saved in `safety_dir` first. The caller must have closed its connection.
pub fn restore(src: &Path, db_path: &Path, safety_dir: &Path) -> Result<(), String> {
    inspect(src)?;
    std::fs::create_dir_all(safety_dir).map_err(|e| e.to_string())?;
    if db_path.exists() {
        let safety = safety_dir.join(format!("before-restore-{}.db", crate::db::now()));
        let current = Connection::open(db_path).map_err(|e| e.to_string())?;
        current
            .execute("VACUUM INTO ?1", [safety.to_string_lossy()])
            .map_err(|e| e.to_string())?;
        drop(current);
    }
    // copy to a temporary file next to the database, then swap
    let tmp = db_path.with_extension("restoring");
    let _ = std::fs::remove_file(&tmp);
    {
        let from = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?;
        from.execute("VACUUM INTO ?1", [tmp.to_string_lossy()])
            .map_err(|e| e.to_string())?;
    }
    for ext in ["db-wal", "db-shm"] {
        let _ = std::fs::remove_file(db_path.with_extension(ext));
    }
    std::fs::rename(&tmp, db_path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::writer::Synthetic;
    use crate::settings::Settings;

    #[test]
    fn backup_and_restore_roundtrip() {
        let dir = std::env::temp_dir().join(format!("fitledger-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("fitledger.db");
        let backup = dir.join("b.fitledger");
        {
            let mut db = Db::open(&db_path, None).unwrap();
            let fit = Synthetic::run(1_100_000_000, 900, 2.8, 150.0, 160.0, true).encode();
            crate::import::import_bytes(&mut db, "a.fit", &fit).unwrap();
            db.save_settings(&Settings {
                age: Some(40),
                ..Default::default()
            })
            .unwrap();
            create(&db, &backup).unwrap();
            // changes after the backup are undone by the restore
            db.conn.execute("DELETE FROM activity", []).unwrap();
            db.save_settings(&Settings::default()).unwrap();
        }
        let info = inspect(&backup).unwrap();
        assert_eq!(info.activities, 1);
        assert_eq!(info.fit_files, 1);
        restore(&backup, &db_path, &dir.join("safety")).unwrap();
        let db = Db::open(&db_path, None).unwrap();
        assert_eq!(db.activities().unwrap().len(), 1);
        assert_eq!(db.load_settings().age, Some(40));
        assert!(std::fs::read_dir(dir.join("safety")).unwrap().count() == 1);
        assert!(inspect(&dir.join("safety")).is_err());
        std::fs::write(dir.join("junk.fitledger"), b"junk").unwrap();
        assert!(inspect(&dir.join("junk.fitledger")).is_err());
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
