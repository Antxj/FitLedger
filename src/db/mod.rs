//! SQLite storage. Three layers:
//! - raw: `fit_file` (original bytes, never modified);
//! - normalized: `activity`, `lap`, `sample`, ... (from the file, plus user edits
//!   kept in separate columns such as `distance_override_m`);
//! - derived: `activity_metrics`, `best_effort` (rebuilt by "Recalculate").

pub mod schema;

use crate::fit::activity::ParsedActivity;
use crate::model::{Activity, Lap, Modality, Sample, TimerEvent};
use crate::settings::Settings;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::Path;

pub type Result<T> = rusqlite::Result<T>;

pub struct Db {
    pub conn: Connection,
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Db {
    /// Opens (or creates) the database and applies pending migrations.
    /// `backup_dir`: where to save a copy before migrating an existing database.
    pub fn open(path: &Path, backup_dir: Option<&Path>) -> Result<Db> {
        let conn = Connection::open(path)?;
        let db = Db::configure(conn)?;
        db.migrate(backup_dir)?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Db> {
        let db = Db::configure(Connection::open_in_memory()?)?;
        db.migrate(None)?;
        Ok(db)
    }

    fn configure(conn: Connection) -> Result<Db> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        Ok(Db { conn })
    }

    pub fn version(&self) -> Result<i64> {
        self.conn.pragma_query_value(None, "user_version", |r| r.get(0))
    }

    fn migrate(&self, backup_dir: Option<&Path>) -> Result<()> {
        let current = self.version()?;
        let latest = schema::latest_version();
        if current > latest {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISMATCH),
                Some(trf!(
                    "O banco de dados é de uma versão mais nova do FitLedger (esquema {current}). \
                     Atualize o aplicativo.",
                    "The database comes from a newer FitLedger version (schema {current}). \
                     Update the app."
                )),
            ));
        }
        if current > 0 && current < latest {
            if let Some(dir) = backup_dir {
                let _ = std::fs::create_dir_all(dir);
                let file = dir.join(format!("before-migration-v{current}-{}.db", now()));
                self.conn.execute("VACUUM INTO ?1", [file.to_string_lossy()])?;
            }
        }
        for (i, sql) in schema::MIGRATIONS.iter().enumerate().skip(current as usize) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ settings --

    pub fn load_settings(&self) -> Settings {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let json = serde_json::to_string(s).unwrap_or_default();
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [json],
        )?;
        Ok(())
    }

    /// Small key/value store for app state (e.g. last update check).
    pub fn get_kv(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_kv(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------- fit files --

    pub fn fit_file_exists(&self, sha256: &str) -> Result<bool> {
        self.conn
            .query_row("SELECT 1 FROM fit_file WHERE sha256 = ?1", [sha256], |_| Ok(()))
            .optional()
            .map(|o| o.is_some())
    }

    pub fn known_hashes(&self) -> Result<std::collections::HashSet<String>> {
        let mut st = self.conn.prepare("SELECT sha256 FROM fit_file")?;
        let rows = st.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    /// Activity that looks like the same workout (re-exported file with another
    /// name): same device + creation time, or same sport starting within 60 s
    /// with a similar duration.
    pub fn find_same_activity(&self, a: &ParsedActivity) -> Result<Option<i64>> {
        if let (Some(serial), Some(created)) = (a.device_serial, a.time_created) {
            let found = self
                .conn
                .query_row(
                    "SELECT id FROM activity WHERE device_serial = ?1 AND fit_time_created = ?2",
                    params![serial, created],
                    |r| r.get(0),
                )
                .optional()?;
            if found.is_some() {
                return Ok(found);
            }
        }
        let elapsed = a.elapsed_s.unwrap_or(0.0);
        self.conn
            .query_row(
                "SELECT id FROM activity
                 WHERE source = 'fit' AND abs(start_time - ?1) <= 60
                   AND coalesce(fit_sport, -1) = coalesce(?2, -1)
                   AND abs(coalesce(elapsed_s, 0) - ?3) <= max(30, ?3 * 0.02)",
                params![a.start_time, a.fit_sport, elapsed],
                |r| r.get(0),
            )
            .optional()
    }

    /// Stores the original file and everything parsed from it, in one transaction.
    pub fn insert_fit_activity(
        &mut self,
        sha256: &str,
        file_name: &str,
        content: &[u8],
        a: &ParsedActivity,
        moving_s: Option<f64>,
    ) -> Result<i64> {
        let ts = now();
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO fit_file(sha256, file_name, size, imported_at, content) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![sha256, file_name, content.len() as i64, ts, content],
        )?;
        let file_id = tx.last_insert_rowid();
        let has_gps = a.samples.iter().any(|s| s.lat.is_some() && s.lon.is_some());
        let tz = a.tz_offset.unwrap_or_else(|| local_offset_at(a.start_time));
        tx.execute(
            "INSERT INTO activity(
                fit_file_id, source, start_time, tz_offset, modality, fit_sport, fit_sub_sport, name,
                elapsed_s, timer_s, moving_s, distance_m, calories, avg_hr, max_hr, min_hr,
                avg_cadence, max_cadence, avg_speed, max_speed, avg_power, max_power,
                ascent_m, descent_m, min_alt, max_alt, avg_temp, max_temp,
                training_effect, anaerobic_te, device, device_serial, fit_time_created, has_gps,
                created_at, updated_at)
             VALUES(?1,'fit',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,
                    ?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?34)",
            params![
                file_id,
                a.start_time,
                tz,
                a.modality.code(),
                a.fit_sport,
                a.fit_sub_sport,
                a.sport_name,
                a.elapsed_s,
                a.timer_s,
                moving_s,
                a.distance_m,
                a.calories,
                a.avg_hr,
                a.max_hr,
                a.min_hr,
                a.avg_cadence,
                a.max_cadence,
                a.avg_speed,
                a.max_speed,
                a.avg_power,
                a.max_power,
                a.ascent_m,
                a.descent_m,
                a.min_alt,
                a.max_alt,
                a.avg_temp,
                a.max_temp,
                a.training_effect,
                a.anaerobic_te,
                a.device,
                a.device_serial,
                a.time_created,
                has_gps,
                ts,
            ],
        )?;
        let id = tx.last_insert_rowid();
        {
            let mut st = tx.prepare(
                "INSERT OR IGNORE INTO sample(activity_id, t, lat, lon, alt, hr, cadence, distance, speed, power, temp)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            )?;
            for s in &a.samples {
                st.execute(params![id, s.t, s.lat, s.lon, s.alt, s.hr, s.cadence, s.distance, s.speed, s.power, s.temp])?;
            }
            let mut st = tx.prepare(
                "INSERT INTO lap(activity_id, idx, start_time, elapsed_s, timer_s, distance_m, calories,
                    avg_speed, max_speed, avg_hr, max_hr, avg_cadence, ascent_m, descent_m, trigger)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            )?;
            for (i, l) in a.laps.iter().enumerate() {
                st.execute(params![
                    id, i as i64, l.start, l.elapsed_s, l.timer_s, l.distance_m, l.calories, l.avg_speed,
                    l.max_speed, l.avg_hr, l.max_hr, l.avg_cadence, l.ascent_m, l.descent_m, l.trigger
                ])?;
            }
            let mut st = tx.prepare(
                "INSERT INTO timer_event(activity_id, t, event, event_type) VALUES(?1,?2,?3,?4)",
            )?;
            for e in &a.events {
                st.execute(params![id, e.t, e.event, e.event_type])?;
            }
            let mut st = tx.prepare(
                "INSERT INTO fit_set(activity_id, idx, t, duration_s, reps, weight_kg, category)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)",
            )?;
            for (i, s) in a.sets.iter().enumerate() {
                st.execute(params![id, i as i64, s.t, s.duration_s, s.reps, s.weight_kg, s.category])?;
            }
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn fit_file_content(&self, file_id: i64) -> Result<(String, Vec<u8>)> {
        self.conn.query_row(
            "SELECT file_name, content FROM fit_file WHERE id = ?1",
            [file_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    // ---------------------------------------------------------- activities --

    pub fn activities(&self) -> Result<Vec<Activity>> {
        let mut st = self.conn.prepare(&format!("{ACTIVITY_SELECT} ORDER BY a.start_time DESC"))?;
        let rows = st.query_map([], activity_from_row)?;
        rows.collect()
    }

    pub fn activity(&self, id: i64) -> Result<Option<Activity>> {
        self.conn
            .query_row(&format!("{ACTIVITY_SELECT} WHERE a.id = ?1"), [id], activity_from_row)
            .optional()
    }

    pub fn samples(&self, activity_id: i64) -> Result<Vec<Sample>> {
        let mut st = self.conn.prepare_cached(
            "SELECT t, lat, lon, alt, hr, cadence, distance, speed, power, temp
             FROM sample WHERE activity_id = ?1 ORDER BY t",
        )?;
        let rows = st.query_map([activity_id], |r| {
            Ok(Sample {
                t: r.get(0)?,
                lat: r.get(1)?,
                lon: r.get(2)?,
                alt: r.get(3)?,
                hr: r.get(4)?,
                cadence: r.get(5)?,
                distance: r.get(6)?,
                speed: r.get(7)?,
                power: r.get(8)?,
                temp: r.get(9)?,
            })
        })?;
        rows.collect()
    }

    pub fn laps(&self, activity_id: i64) -> Result<Vec<Lap>> {
        let mut st = self.conn.prepare_cached(
            "SELECT start_time, elapsed_s, timer_s, distance_m, calories, avg_speed, max_speed,
                    avg_hr, max_hr, avg_cadence, ascent_m, descent_m, trigger
             FROM lap WHERE activity_id = ?1 ORDER BY idx",
        )?;
        let rows = st.query_map([activity_id], |r| {
            Ok(Lap {
                start: r.get(0)?,
                elapsed_s: r.get(1)?,
                timer_s: r.get(2)?,
                distance_m: r.get(3)?,
                calories: r.get(4)?,
                avg_speed: r.get(5)?,
                max_speed: r.get(6)?,
                avg_hr: r.get(7)?,
                max_hr: r.get(8)?,
                avg_cadence: r.get(9)?,
                ascent_m: r.get(10)?,
                descent_m: r.get(11)?,
                trigger: r.get(12)?,
            })
        })?;
        rows.collect()
    }

    pub fn timer_events(&self, activity_id: i64) -> Result<Vec<TimerEvent>> {
        let mut st = self.conn.prepare_cached(
            "SELECT t, event, event_type FROM timer_event WHERE activity_id = ?1 ORDER BY t",
        )?;
        let rows = st.query_map([activity_id], |r| {
            Ok(TimerEvent { t: r.get(0)?, event: r.get(1)?, event_type: r.get(2)? })
        })?;
        rows.collect()
    }

    /// Saves the user-editable fields of an activity.
    pub fn update_activity_user_fields(&self, a: &Activity) -> Result<()> {
        self.conn.execute(
            "UPDATE activity SET modality = ?2, name = ?3, distance_override_m = ?4, incline_pct = ?5,
                notes = ?6, rpe = ?7, sleep_quality = ?8, fatigue = ?9, soreness = ?10, body_weight = ?11,
                updated_at = ?12
             WHERE id = ?1",
            params![
                a.id,
                a.modality.code(),
                a.name,
                a.distance_override_m,
                a.incline_pct,
                a.notes,
                a.rpe,
                a.sleep_quality,
                a.fatigue,
                a.soreness,
                a.body_weight,
                now()
            ],
        )?;
        Ok(())
    }

    /// Manual activity (no FIT file) or edit of one. Returns the id.
    pub fn save_manual_activity(&self, a: &Activity) -> Result<i64> {
        let ts = now();
        if a.id == 0 {
            self.conn.execute(
                "INSERT INTO activity(source, start_time, tz_offset, modality, name, elapsed_s, timer_s,
                    distance_m, calories, avg_hr, max_hr, incline_pct, notes, rpe, sleep_quality, fatigue,
                    soreness, body_weight, created_at, updated_at)
                 VALUES('manual',?1,?2,?3,?4,?5,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?17)",
                params![
                    a.start_time, a.tz_offset, a.modality.code(), a.name, a.elapsed_s, a.distance_m,
                    a.calories, a.avg_hr, a.max_hr, a.incline_pct, a.notes, a.rpe, a.sleep_quality,
                    a.fatigue, a.soreness, a.body_weight, ts
                ],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE activity SET start_time = ?2, tz_offset = ?3, modality = ?4, name = ?5,
                    elapsed_s = ?6, timer_s = ?6, distance_m = ?7, calories = ?8, avg_hr = ?9, max_hr = ?10,
                    incline_pct = ?11, notes = ?12, rpe = ?13, sleep_quality = ?14, fatigue = ?15,
                    soreness = ?16, body_weight = ?17, updated_at = ?18
                 WHERE id = ?1 AND source = 'manual'",
                params![
                    a.id, a.start_time, a.tz_offset, a.modality.code(), a.name, a.elapsed_s, a.distance_m,
                    a.calories, a.avg_hr, a.max_hr, a.incline_pct, a.notes, a.rpe, a.sleep_quality,
                    a.fatigue, a.soreness, a.body_weight, ts
                ],
            )?;
            Ok(a.id)
        }
    }

    /// Deletes an activity. The original FIT file is deleted too, so that it
    /// can be imported again from /fits if the user changes their mind
    /// (the file on disk is never touched).
    pub fn delete_activity(&mut self, id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        let file: Option<i64> = tx
            .query_row("SELECT fit_file_id FROM activity WHERE id = ?1", [id], |r| r.get(0))
            .optional()?
            .flatten();
        tx.execute("UPDATE planned_workout SET activity_id = NULL WHERE activity_id = ?1", [id])?;
        tx.execute("DELETE FROM activity WHERE id = ?1", [id])?;
        if let Some(f) = file {
            tx.execute("DELETE FROM fit_file WHERE id = ?1", [f])?;
        }
        tx.commit()
    }

    pub fn activity_ids(&self) -> Result<Vec<i64>> {
        let mut st = self.conn.prepare("SELECT id FROM activity ORDER BY start_time")?;
        let rows = st.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    /// Highest heart rate in all activities (to estimate the max HR).
    pub fn observed_max_hr(&self) -> Option<f64> {
        self.conn
            .query_row("SELECT max(max_hr) FROM activity WHERE max_hr BETWEEN 100 AND 230", [], |r| r.get(0))
            .ok()
            .flatten()
    }
}

const ACTIVITY_SELECT: &str = "SELECT a.id, a.fit_file_id, a.source, a.start_time, a.tz_offset, a.modality,
    a.fit_sport, a.fit_sub_sport, a.name, a.elapsed_s, a.timer_s, a.moving_s, a.distance_m,
    a.distance_override_m, a.calories, a.avg_hr, a.max_hr, a.min_hr, a.avg_cadence, a.max_cadence,
    a.avg_speed, a.max_speed, a.avg_power, a.max_power, a.ascent_m, a.descent_m, a.min_alt, a.max_alt,
    a.avg_temp, a.max_temp, a.training_effect, a.anaerobic_te, a.device, a.has_gps, a.incline_pct,
    a.notes, a.rpe, a.sleep_quality, a.fatigue, a.soreness, a.body_weight, m.load, m.drift_pct
    FROM activity a LEFT JOIN activity_metrics m ON m.activity_id = a.id";

fn activity_from_row(r: &Row) -> Result<Activity> {
    Ok(Activity {
        id: r.get(0)?,
        fit_file_id: r.get(1)?,
        source: r.get(2)?,
        start_time: r.get(3)?,
        tz_offset: r.get(4)?,
        modality: Modality::from_code(&r.get::<_, String>(5)?),
        fit_sport: r.get(6)?,
        fit_sub_sport: r.get(7)?,
        name: r.get(8)?,
        elapsed_s: r.get(9)?,
        timer_s: r.get(10)?,
        moving_s: r.get(11)?,
        distance_m: r.get(12)?,
        distance_override_m: r.get(13)?,
        calories: r.get(14)?,
        avg_hr: r.get(15)?,
        max_hr: r.get(16)?,
        min_hr: r.get(17)?,
        avg_cadence: r.get(18)?,
        max_cadence: r.get(19)?,
        avg_speed: r.get(20)?,
        max_speed: r.get(21)?,
        avg_power: r.get(22)?,
        max_power: r.get(23)?,
        ascent_m: r.get(24)?,
        descent_m: r.get(25)?,
        min_alt: r.get(26)?,
        max_alt: r.get(27)?,
        avg_temp: r.get(28)?,
        max_temp: r.get(29)?,
        training_effect: r.get(30)?,
        anaerobic_te: r.get(31)?,
        device: r.get(32)?,
        has_gps: r.get(33)?,
        incline_pct: r.get(34)?,
        notes: r.get(35)?,
        rpe: r.get(36)?,
        sleep_quality: r.get(37)?,
        fatigue: r.get(38)?,
        soreness: r.get(39)?,
        body_weight: r.get(40)?,
        load: r.get(41)?,
        drift_pct: r.get(42)?,
    })
}

/// Offset of the computer's local time zone at a given instant (for files
/// that do not record the local time).
pub fn local_offset_at(unix: i64) -> i64 {
    use chrono::{Local, Offset, TimeZone};
    match Local.timestamp_opt(unix, 0) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            dt.offset().fix().local_minus_utc() as i64
        }
        chrono::LocalResult::None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_has_latest_schema() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.version().unwrap(), schema::latest_version());
        assert!(db.activities().unwrap().is_empty());
    }

    #[test]
    fn settings_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.load_settings(), Settings::default());
        let s = Settings { age: Some(38), hr_max: Some(195.0), onboarded: true, ..Default::default() };
        db.save_settings(&s).unwrap();
        assert_eq!(db.load_settings(), s);
    }

    #[test]
    fn reopening_a_file_database_keeps_data_and_backs_up_before_migrating() {
        let dir = std::env::temp_dir().join(format!("fitledger-db-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.db");
        {
            let db = Db::open(&path, Some(&dir)).unwrap();
            db.save_settings(&Settings { age: Some(30), ..Default::default() }).unwrap();
        }
        let db = Db::open(&path, Some(&dir)).unwrap();
        assert_eq!(db.load_settings().age, Some(30));
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
