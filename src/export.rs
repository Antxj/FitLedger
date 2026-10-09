//! Export to CSV and JSON (item 32): full history, runs, strength, tennis,
//! goals and monthly summaries.

use crate::analysis::metrics::Metrics;
use crate::analysis::trends::{self, Bucket};
use crate::db::Db;
use crate::model::{Activity, Modality};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    All,
    Runs,
    Strength,
    Tennis,
    Goals,
    Monthly,
}

impl What {
    pub const ALL: [What; 6] = [
        What::All,
        What::Runs,
        What::Strength,
        What::Tennis,
        What::Goals,
        What::Monthly,
    ];

    pub fn label(self) -> &'static str {
        match self {
            What::All => tr!("Histórico completo", "Full history"),
            What::Runs => tr!("Corridas", "Runs"),
            What::Strength => tr!("Musculação", "Strength"),
            What::Tennis => tr!("Tênis", "Tennis"),
            What::Goals => tr!("Metas", "Goals"),
            What::Monthly => tr!("Resumos mensais", "Monthly summaries"),
        }
    }

    pub fn file_stem(self) -> &'static str {
        match self {
            What::All => "fitledger-history",
            What::Runs => "fitledger-runs",
            What::Strength => "fitledger-strength",
            What::Tennis => "fitledger-tennis",
            What::Goals => "fitledger-goals",
            What::Monthly => "fitledger-monthly",
        }
    }
}

/// A table: header + rows of JSON values (numbers stay numbers in JSON).
pub struct Table {
    pub header: Vec<&'static str>,
    pub rows: Vec<Vec<Value>>,
}

fn round(v: Option<f64>, d: i32) -> Value {
    match v {
        Some(x) if x.is_finite() => {
            let k = 10f64.powi(d);
            json!((x * k).round() / k)
        }
        _ => Value::Null,
    }
}

fn activity_row(a: &Activity, m: Option<&Metrics>) -> Vec<Value> {
    let ls = a.local_start();
    vec![
        json!(a.id),
        json!(ls.format("%Y-%m-%d").to_string()),
        json!(ls.format("%H:%M").to_string()),
        json!(a.modality.code()),
        json!(a.title()),
        round(a.distance().map(|d| d / 1000.0), 3),
        round(a.duration(), 0),
        round(a.elapsed_s, 0),
        a.pace_s_per_km()
            .filter(|_| a.modality.is_run())
            .map(|p| json!(crate::format::pace(p)))
            .unwrap_or(Value::Null),
        round(a.avg_speed.map(|v| v * 3.6), 2),
        round(a.avg_hr, 0),
        round(a.max_hr, 0),
        round(a.avg_cadence, 0),
        round(a.ascent_m.or(m.and_then(|m| m.gain_m)), 0),
        round(a.descent_m.or(m.and_then(|m| m.loss_m)), 0),
        round(a.calories, 0),
        round(m.map(|m| m.load), 0),
        round(a.drift_pct, 1),
        round(a.incline_pct, 1),
        json!(a.rpe),
        json!(a.sleep_quality),
        json!(a.fatigue),
        json!(a.soreness),
        round(a.body_weight, 1),
        json!(a.device),
        json!(a.source),
        json!(a.notes.clone().unwrap_or_default()),
    ]
}

const ACTIVITY_HEADER: [&str; 27] = [
    "id",
    "date",
    "time",
    "modality",
    "title",
    "distance_km",
    "duration_s",
    "elapsed_s",
    "pace_min_km",
    "avg_speed_kmh",
    "avg_hr",
    "max_hr",
    "avg_cadence_spm",
    "ascent_m",
    "descent_m",
    "calories",
    "load",
    "decoupling_pct",
    "incline_pct",
    "rpe",
    "sleep_quality",
    "fatigue",
    "soreness",
    "body_weight_kg",
    "device",
    "source",
    "notes",
];

pub fn build(db: &Db, what: What, acts: &[Activity], metrics: &HashMap<i64, Metrics>) -> Table {
    let pick = |f: &dyn Fn(Modality) -> bool| -> Table {
        let mut rows: Vec<&Activity> = acts.iter().filter(|a| f(a.modality)).collect();
        rows.sort_by_key(|a| a.start_time);
        Table {
            header: ACTIVITY_HEADER.to_vec(),
            rows: rows.iter().map(|a| activity_row(a, metrics.get(&a.id))).collect(),
        }
    };
    match what {
        What::All => pick(&|_| true),
        What::Runs => pick(&|m| m.is_run()),
        What::Tennis => pick(&|m| m == Modality::Tennis),
        What::Strength => {
            let names: HashMap<i64, String> = db
                .exercises()
                .unwrap_or_default()
                .into_iter()
                .map(|e| (e.id, e.name))
                .collect();
            let mut rows = Vec::new();
            let mut strength: Vec<&Activity> = acts.iter().filter(|a| a.modality == Modality::Strength).collect();
            strength.sort_by_key(|a| a.start_time);
            for a in strength {
                let groups = db.set_groups(a.id).unwrap_or_default();
                for g in groups {
                    rows.push(vec![
                        json!(a.id),
                        json!(a.local_start().format("%Y-%m-%d").to_string()),
                        json!(names.get(&g.exercise_id).cloned().unwrap_or_default()),
                        json!(g.sets),
                        json!(g.reps),
                        round(g.weight_kg, 2),
                        round(Some(g.volume()), 1),
                        round(g.rpe, 1),
                        round(g.rest_s, 0),
                        json!(g.notes),
                    ]);
                }
                if db.set_groups(a.id).map(|g| g.is_empty()).unwrap_or(true) {
                    rows.push(vec![
                        json!(a.id),
                        json!(a.local_start().format("%Y-%m-%d").to_string()),
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        Value::Null,
                        json!(a.rpe),
                        Value::Null,
                        json!(a.notes.clone().unwrap_or_default()),
                    ]);
                }
            }
            Table {
                header: vec![
                    "activity_id",
                    "date",
                    "exercise",
                    "sets",
                    "reps",
                    "weight_kg",
                    "volume_kg",
                    "rpe",
                    "rest_s",
                    "notes",
                ],
                rows,
            }
        }
        What::Goals => {
            let today = chrono::Local::now().date_naive();
            let be = best_efforts_map(db);
            let rows = db
                .goals()
                .unwrap_or_default()
                .into_iter()
                .map(|g| {
                    let p = crate::analysis::goals::progress(&g, acts, &be, today);
                    vec![
                        json!(g.id),
                        json!(g.title),
                        json!(g.kind.code()),
                        json!(serde_json::to_value(&g.params).unwrap_or(Value::Null))
                            .to_string()
                            .into(),
                        json!(g.start_date.map(crate::planning::date_str)),
                        json!(g.end_date.map(crate::planning::date_str)),
                        json!(g.objective_id),
                        round(Some(p.fraction as f64 * 100.0), 1),
                        json!(p.done),
                        json!(p.text),
                        json!(g.archived),
                    ]
                })
                .collect();
            Table {
                header: vec![
                    "id",
                    "title",
                    "kind",
                    "params",
                    "start",
                    "end",
                    "objective_id",
                    "progress_pct",
                    "done",
                    "status",
                    "archived",
                ],
                rows,
            }
        }
        What::Monthly => {
            let vols = db.strength_volumes().unwrap_or_default();
            let today = chrono::Local::now().date_naive();
            let b = trends::buckets(acts, metrics, &vols, None, today, Bucket::Month);
            let rows = b
                .into_iter()
                .map(|(month, t)| {
                    vec![
                        json!(month.format("%Y-%m").to_string()),
                        json!(t.sessions),
                        round(Some(t.run_km), 2),
                        round(Some(t.hours), 2),
                        t.avg_pace()
                            .map(|p| json!(crate::format::pace(p)))
                            .unwrap_or(Value::Null),
                        round(t.avg_hr(), 0),
                        round(Some(t.longest_km), 2),
                        json!(t.strength_sessions),
                        round(Some(t.strength_volume), 0),
                        round(Some(t.tennis_h), 2),
                        round(Some(t.load), 0),
                    ]
                })
                .collect();
            Table {
                header: vec![
                    "month",
                    "sessions",
                    "run_km",
                    "hours",
                    "avg_run_pace",
                    "avg_run_hr",
                    "longest_run_km",
                    "strength_sessions",
                    "strength_volume_kg",
                    "tennis_hours",
                    "load",
                ],
                rows,
            }
        }
    }
}

pub fn best_efforts_map(db: &Db) -> HashMap<i64, Vec<(f64, f64)>> {
    let mut out: HashMap<i64, Vec<(f64, f64)>> = HashMap::new();
    if let Ok(mut st) = db
        .conn
        .prepare("SELECT activity_id, distance_m, time_s FROM best_effort")
        && let Ok(rows) = st.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?, r.get::<_, f64>(2)?))
        })
    {
        for (id, d, t) in rows.flatten() {
            out.entry(id).or_default().push((d, t));
        }
    }
    out
}

fn csv_cell(v: &Value) -> String {
    let s = match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if s.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

/// CSV with comma separator and UTF-8 BOM (opens correctly in Excel).
pub fn to_csv(t: &Table) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str(&t.header.join(","));
    out.push_str("\r\n");
    for r in &t.rows {
        out.push_str(&r.iter().map(csv_cell).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

pub fn to_json(t: &Table) -> String {
    let rows: Vec<Value> = t
        .rows
        .iter()
        .map(|r| {
            let obj: serde_json::Map<String, Value> = t
                .header
                .iter()
                .zip(r.iter())
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect();
            Value::Object(obj)
        })
        .collect();
    serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escaping_and_json_shape() {
        let t = Table {
            header: vec!["a", "b", "c"],
            rows: vec![vec![json!(1), json!("x, \"y\""), Value::Null]],
        };
        let csv = to_csv(&t);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("1,\"x, \"\"y\"\"\",\r\n"));
        let j: Vec<Value> = serde_json::from_str(&to_json(&t)).unwrap();
        assert_eq!(j[0]["a"], json!(1));
        assert_eq!(j[0]["c"], Value::Null);
    }

    #[test]
    fn export_activities_from_db() {
        let mut db = Db::open_in_memory().unwrap();
        let fit = crate::fit::writer::Synthetic::run(1_100_000_000, 900, 2.8, 150.0, 160.0, true).encode();
        crate::import::import_bytes(&mut db, "a.fit", &fit).unwrap();
        let acts = db.activities().unwrap();
        let m = crate::analysis::metrics::all_metrics(&db);
        for w in What::ALL {
            let t = build(&db, w, &acts, &m);
            assert!(t.rows.iter().all(|r| r.len() == t.header.len()), "{:?}", w.label());
        }
        let runs = build(&db, What::Runs, &acts, &m);
        assert_eq!(runs.rows.len(), 1);
        assert_eq!(runs.rows[0][3], json!("road_run"));
    }
}
