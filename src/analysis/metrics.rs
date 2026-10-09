//! Derived metrics of one activity, stored in `activity_metrics` (JSON) and
//! `best_effort`. Everything here can be deleted and rebuilt from the
//! normalized data ("Recalculate metrics", item 37).

use super::hr::{self, Drift};
use super::splits::{self, RECORD_DISTANCES};
use super::stream::{QualityReport, Stream};
use crate::db::Db;
use crate::model::{Activity, Modality};
use crate::settings::Settings;
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// Bump when an algorithm changes: the app recalculates on the next start.
pub const METRICS_VERSION: i64 = 1;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metrics {
    pub moving_s: Option<f64>,
    pub pauses: Vec<(f64, f64)>,
    /// Seconds below zone 1 and in zones 1..5.
    pub zones_s: [f64; 6],
    /// Lower bound of zones 1..5 used in this calculation, and max HR.
    pub zone_lower: [f64; 5],
    pub zone_max: f64,
    pub drift: Option<Drift>,
    pub load: f64,
    /// "hr" | "rpe" | "estimate"
    pub load_source: String,
    pub efficiency: Option<f64>,
    pub quality: QualityReport,
    /// (pace s/km, HR) of steady stretches.
    pub pace_hr: Vec<[f64; 2]>,
    /// Elevation computed from altitude (when the file has none).
    pub gain_m: Option<f64>,
    pub loss_m: Option<f64>,
    /// Average HR of each full km (reference tests).
    pub km_hr: Vec<Option<f64>>,
    /// Share of time in zones 4 and 5.
    pub hard_share: f64,
}

impl Metrics {
    /// Hard session: lots of time in zones 4–5, or high load per minute, or RPE ≥ 8.
    pub fn is_hard(&self, a: &Activity) -> bool {
        let minutes = a.duration().unwrap_or(0.0) / 60.0;
        self.hard_share >= 0.25
            || (minutes > 0.0 && self.load / minutes >= 2.2)
            || a.rpe.is_some_and(|r| r >= 8)
    }
}

/// Correction factor for treadmill distance typed by the user.
pub fn distance_scale(a: &Activity) -> f64 {
    match (a.distance_override_m, a.distance_m) {
        (Some(o), Some(d)) if d > 0.0 && o > 0.0 => o / d,
        _ => 1.0,
    }
}

pub fn stream_for(db: &Db, a: &Activity) -> rusqlite::Result<Stream> {
    let samples = db.samples(a.id)?;
    Ok(Stream::build(&samples, a.modality, distance_scale(a)))
}

pub fn compute(a: &Activity, st: &Stream, settings: &Settings, observed_max: Option<f64>) -> Metrics {
    let zones = settings.zones(observed_max);
    let mut m = Metrics {
        zone_lower: zones.lower,
        zone_max: zones.max,
        quality: st.quality.clone(),
        ..Default::default()
    };
    let minutes = a.duration().unwrap_or(0.0) / 60.0;
    if st.is_empty() {
        m.load = hr::load_without_hr(minutes, a.rpe);
        m.load_source = if a.rpe.is_some() { "rpe" } else { "estimate" }.into();
        return m;
    }
    m.moving_s = Some(st.moving_time()).filter(|&v| v > 0.0);
    m.pauses = st.pauses();
    m.zones_s = hr::time_in_zones(st, &zones);
    let total_z: f64 = m.zones_s.iter().sum();
    if total_z > 0.0 {
        m.hard_share = (m.zones_s[4] + m.zones_s[5]) / total_z;
    }
    match hr::trimp(st, settings.effective_hr_rest(), zones.max) {
        Some(t) => {
            m.load = t;
            m.load_source = "hr".into();
        }
        None => {
            m.load = hr::load_without_hr(minutes, a.rpe);
            m.load_source = if a.rpe.is_some() { "rpe" } else { "estimate" }.into();
        }
    }
    if a.modality.is_run() {
        m.drift = hr::cardiac_drift(st, 0.0);
        m.efficiency = hr::efficiency(st);
        m.pace_hr = hr::pace_hr_points(st);
        let km = splits::splits(st, 1000.0);
        m.km_hr = km.iter().filter(|s| s.distance_m >= 900.0).map(|s| s.avg_hr).collect();
    }
    if st.alt.iter().any(Option::is_some) {
        let (g, l) = st.elevation(0, st.len());
        m.gain_m = Some(g);
        m.loss_m = Some(l);
    }
    m
}

/// Computes and stores the metrics (and best efforts) of one activity.
pub fn update_activity(db: &Db, a: &Activity, settings: &Settings, observed_max: Option<f64>) -> rusqlite::Result<Metrics> {
    let st = stream_for(db, a)?;
    let m = compute(a, &st, settings, observed_max);
    let json = serde_json::to_string(&m).unwrap_or_else(|_| "{}".into());
    let tx = db.conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO activity_metrics(activity_id, version, load, drift_pct, efficiency, flagged, json)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(activity_id) DO UPDATE SET version = excluded.version, load = excluded.load,
            drift_pct = excluded.drift_pct, efficiency = excluded.efficiency,
            flagged = excluded.flagged, json = excluded.json",
        params![
            a.id,
            METRICS_VERSION,
            m.load,
            m.drift.as_ref().and_then(|d| d.decoupling_pct),
            m.efficiency,
            m.quality.total(),
            json
        ],
    )?;
    if m.moving_s.is_some() && a.source == "fit" {
        tx.execute("UPDATE activity SET moving_s = ?2 WHERE id = ?1", params![a.id, m.moving_s])?;
    }
    tx.execute("DELETE FROM best_effort WHERE activity_id = ?1", [a.id])?;
    if a.modality.is_run() {
        for d in RECORD_DISTANCES {
            if let Some((time, start)) = splits::best_effort(&st, d) {
                tx.execute(
                    "INSERT INTO best_effort(activity_id, distance_m, time_s, start_s) VALUES(?1, ?2, ?3, ?4)",
                    params![a.id, d, time, start],
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(m)
}

pub fn load_metrics(db: &Db, activity_id: i64) -> Option<Metrics> {
    db.conn
        .query_row("SELECT json FROM activity_metrics WHERE activity_id = ?1", [activity_id], |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .and_then(|j| serde_json::from_str(&j).ok())
}

/// All metrics, keyed by activity id (for pages that look at many activities).
pub fn all_metrics(db: &Db) -> std::collections::HashMap<i64, Metrics> {
    let mut out = std::collections::HashMap::new();
    if let Ok(mut st) = db.conn.prepare("SELECT activity_id, json FROM activity_metrics") {
        if let Ok(rows) = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))) {
            for (id, j) in rows.flatten() {
                if let Ok(m) = serde_json::from_str(&j) {
                    out.insert(id, m);
                }
            }
        }
    }
    out
}

/// Recalculates everything; `progress(done, total)`. Returns how many were processed.
pub fn recalculate_all(db: &Db, progress: &mut dyn FnMut(usize, usize)) -> rusqlite::Result<usize> {
    let settings = db.load_settings();
    let observed = db.observed_max_hr();
    let acts = db.activities()?;
    let total = acts.len();
    for (i, a) in acts.iter().enumerate() {
        update_activity(db, a, &settings, observed)?;
        progress(i + 1, total);
    }
    db.set_kv("metrics_version", &METRICS_VERSION.to_string())?;
    Ok(total)
}

/// True when stored metrics were made by an older algorithm (or are missing).
pub fn needs_recalculation(db: &Db) -> bool {
    let stale: i64 = db
        .conn
        .query_row(
            "SELECT count(*) FROM activity a LEFT JOIN activity_metrics m ON m.activity_id = a.id
             WHERE m.version IS NULL OR m.version <> ?1",
            [METRICS_VERSION],
            |r| r.get(0),
        )
        .unwrap_or(0);
    stale > 0
}

/// Terrain class from elevation gain per km (item 39).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Terrain {
    Indoor,
    Flat,
    Rolling,
    Hilly,
    Unknown,
}

impl Terrain {
    pub fn of(a: &Activity, m: Option<&Metrics>) -> Terrain {
        if a.modality == Modality::Treadmill {
            return Terrain::Indoor;
        }
        let gain = a.ascent_m.or_else(|| m.and_then(|m| m.gain_m));
        match (gain, a.distance()) {
            (Some(g), Some(d)) if d > 500.0 => {
                let per_km = g / (d / 1000.0);
                if per_km < 6.0 {
                    Terrain::Flat
                } else if per_km < 15.0 {
                    Terrain::Rolling
                } else {
                    Terrain::Hilly
                }
            }
            _ => Terrain::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Terrain::Indoor => tr!("esteira", "treadmill"),
            Terrain::Flat => tr!("plano", "flat"),
            Terrain::Rolling => tr!("ondulado", "rolling"),
            Terrain::Hilly => tr!("com subidas", "hilly"),
            Terrain::Unknown => tr!("relevo desconhecido", "unknown terrain"),
        }
    }
}
