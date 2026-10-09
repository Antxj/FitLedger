//! Analyses across activities: pace × heart rate (item 8), period buckets
//! for the Progress page (item 24) and comparability (item 39).

use super::load::{day_of, week_start};
use super::metrics::{Metrics, Terrain};
use crate::model::{Activity, Modality};
use chrono::{Duration, NaiveDate};
use std::collections::{BTreeMap, HashMap};

/// Width of the pace bands, s/km.
pub const PACE_BIN: f64 = 15.0;

/// Lower edge of the 15 s band containing `pace` (e.g. 6:07 → 6:00).
pub fn pace_band(pace: f64) -> f64 {
    (pace / PACE_BIN).floor() * PACE_BIN
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaceBand {
    /// Lower edge, s/km (band = [pace, pace + 15)).
    pub pace: f64,
    pub avg_hr: f64,
    /// Steady minutes found in this band.
    pub minutes: usize,
    pub activities: usize,
}

/// Steady stretches grouped by pace band (all runs matching the filter).
pub fn pace_bands<'a>(acts: impl Iterator<Item = &'a Activity>, metrics: &HashMap<i64, Metrics>) -> Vec<PaceBand> {
    let mut map: BTreeMap<i64, (f64, usize, std::collections::HashSet<i64>)> = BTreeMap::new();
    for a in acts {
        let Some(m) = metrics.get(&a.id) else { continue };
        for p in &m.pace_hr {
            let key = pace_band(p[0]) as i64;
            let e = map.entry(key).or_insert((0.0, 0, Default::default()));
            e.0 += p[1];
            e.1 += 1;
            e.2.insert(a.id);
        }
    }
    map.into_iter()
        .map(|(k, (sum, n, ids))| PaceBand { pace: k as f64, avg_hr: sum / n as f64, minutes: n, activities: ids.len() })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct HrAtPace {
    pub activity_id: i64,
    pub date: NaiveDate,
    pub hr: f64,
    /// Steady minutes used (0 = whole-activity average).
    pub minutes: usize,
    pub modality: Modality,
}

/// Heart rate at a given pace over time: per run, the average HR of the
/// steady stretches within ±7.5 s/km of `pace`. Runs without steady
/// stretches use their overall average when their average pace is close.
pub fn hr_at_pace<'a>(acts: impl Iterator<Item = &'a Activity>, metrics: &HashMap<i64, Metrics>, pace: f64) -> Vec<HrAtPace> {
    let tol = PACE_BIN / 2.0;
    let mut out = Vec::new();
    for a in acts {
        if !a.modality.is_run() {
            continue;
        }
        let pts: Vec<f64> = metrics
            .get(&a.id)
            .map(|m| m.pace_hr.iter().filter(|p| (p[0] - pace).abs() <= tol).map(|p| p[1]).collect())
            .unwrap_or_default();
        if pts.len() >= 3 {
            out.push(HrAtPace {
                activity_id: a.id,
                date: day_of(a),
                hr: pts.iter().sum::<f64>() / pts.len() as f64,
                minutes: pts.len(),
                modality: a.modality,
            });
        } else if let (Some(p), Some(h)) = (a.pace_s_per_km(), a.avg_hr) {
            if (p - pace).abs() <= tol {
                out.push(HrAtPace { activity_id: a.id, date: day_of(a), hr: h, minutes: 0, modality: a.modality });
            }
        }
    }
    out.sort_by_key(|x| x.date);
    out
}

/// Average pace of the steady stretches with HR inside [lo, hi].
pub fn pace_in_hr_band(m: &Metrics, lo: f64, hi: f64) -> Option<(f64, usize)> {
    let pts: Vec<f64> = m.pace_hr.iter().filter(|p| p[1] >= lo && p[1] <= hi).map(|p| p[0]).collect();
    (pts.len() >= 3).then(|| (pts.iter().sum::<f64>() / pts.len() as f64, pts.len()))
}

/// Most common pace band of the steady stretches (the default of the pace × HR page).
pub fn typical_pace(bands: &[PaceBand]) -> Option<f64> {
    bands.iter().max_by_key(|b| b.minutes).map(|b| b.pace + PACE_BIN / 2.0)
}

/// Differences that make a direct comparison misleading (item 39).
pub fn comparability_notes(acts: &[&Activity], metrics: &HashMap<i64, Metrics>) -> Vec<String> {
    let mut notes = Vec::new();
    if acts.len() < 2 {
        return notes;
    }
    let mods: std::collections::HashSet<Modality> = acts.iter().map(|a| a.modality).collect();
    if mods.contains(&Modality::Treadmill) && mods.contains(&Modality::RoadRun) {
        notes.push(tr!(
            "Mistura esteira e rua: na esteira não há vento nem variação de terreno, e a distância é estimada pelo relógio.",
            "Mixes treadmill and road: the treadmill has no wind or terrain changes, and its distance is estimated by the watch."
        ).to_string());
    }
    let terrains: std::collections::HashSet<Terrain> =
        acts.iter().map(|a| Terrain::of(a, metrics.get(&a.id))).filter(|t| !matches!(t, Terrain::Unknown | Terrain::Indoor)).collect();
    if terrains.len() > 1 {
        let list: Vec<String> = acts
            .iter()
            .map(|a| {
                let gain = a.ascent_m.or_else(|| metrics.get(&a.id).and_then(|m| m.gain_m)).unwrap_or(0.0);
                format!("{} {} (+{} m)", crate::format::date_short(day_of(a)), Terrain::of(a, metrics.get(&a.id)).label(), gain.round())
            })
            .collect();
        notes.push(trf!(
            "Relevos diferentes: {}. Subidas elevam a FC no mesmo pace.",
            "Different terrain: {}. Climbs raise HR at the same pace.",
            list.join(", ")
        ));
    }
    let dists: Vec<f64> = acts.iter().filter_map(|a| a.distance()).collect();
    if dists.len() >= 2 {
        let (lo, hi) = (dists.iter().cloned().fold(f64::MAX, f64::min), dists.iter().cloned().fold(0.0, f64::max));
        if lo > 0.0 && hi / lo > 1.3 {
            notes.push(trf!(
                "Distâncias bem diferentes ({} a {} km): o pace e a FC mudam com a duração do esforço.",
                "Quite different distances ({} to {} km): pace and HR change with the effort's duration.",
                crate::format::num(lo / 1000.0, 1),
                crate::format::num(hi / 1000.0, 1)
            ));
        }
    }
    let temps: Vec<f64> = acts.iter().filter_map(|a| a.avg_temp).collect();
    if temps.len() >= 2 {
        let (lo, hi) = (temps.iter().cloned().fold(f64::MAX, f64::min), temps.iter().cloned().fold(f64::MIN, f64::max));
        if hi - lo >= 6.0 {
            notes.push(trf!(
                "Temperaturas diferentes ({} a {} °C): calor eleva a FC.",
                "Different temperatures ({} to {} °C): heat raises HR.",
                lo.round(),
                hi.round()
            ));
        }
    }
    notes
}

// ------------------------------------------------------- Progress buckets --

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Day,
    Week,
    Month,
}

pub fn bucket_start(d: NaiveDate, b: Bucket) -> NaiveDate {
    use chrono::Datelike;
    match b {
        Bucket::Day => d,
        Bucket::Week => week_start(d),
        Bucket::Month => d.with_day(1).unwrap_or(d),
    }
}

/// Totals per bucket, for the Progress charts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Totals {
    pub run_km: f64,
    pub hours: f64,
    pub run_s: f64,
    pub run_m: f64,
    pub run_hr_sum: f64,
    pub run_hr_n: f64,
    pub longest_km: f64,
    pub strength_volume: f64,
    pub strength_sessions: f64,
    pub tennis_h: f64,
    pub load: f64,
    pub sessions: f64,
}

impl Totals {
    pub fn avg_pace(&self) -> Option<f64> {
        (self.run_m > 100.0).then(|| self.run_s / (self.run_m / 1000.0))
    }

    pub fn avg_hr(&self) -> Option<f64> {
        (self.run_hr_n > 0.0).then(|| self.run_hr_sum / self.run_hr_n)
    }
}

pub fn buckets(
    acts: &[Activity],
    metrics: &HashMap<i64, Metrics>,
    strength_volume: &HashMap<i64, f64>,
    from: Option<NaiveDate>,
    to: NaiveDate,
    b: Bucket,
) -> BTreeMap<NaiveDate, Totals> {
    let mut out: BTreeMap<NaiveDate, Totals> = BTreeMap::new();
    // empty buckets too, so the charts show the gaps
    if let Some(f) = from {
        let mut d = bucket_start(f, b);
        while d <= to {
            out.entry(d).or_default();
            d = match b {
                Bucket::Day => d + Duration::days(1),
                Bucket::Week => d + Duration::days(7),
                Bucket::Month => {
                    use chrono::Months;
                    d.checked_add_months(Months::new(1)).unwrap_or(to + Duration::days(1))
                }
            };
        }
    }
    for a in acts {
        let day = day_of(a);
        if from.is_some_and(|f| day < f) || day > to {
            continue;
        }
        let t = out.entry(bucket_start(day, b)).or_default();
        let dur = a.duration().unwrap_or(0.0);
        t.hours += dur / 3600.0;
        t.sessions += 1.0;
        t.load += super::load::activity_load(a, metrics.get(&a.id));
        match a.modality {
            Modality::RoadRun | Modality::Treadmill => {
                let d = a.distance().unwrap_or(0.0);
                t.run_km += d / 1000.0;
                if d > 100.0 {
                    t.run_m += d;
                    t.run_s += dur;
                }
                t.longest_km = t.longest_km.max(d / 1000.0);
                if let Some(h) = a.avg_hr {
                    t.run_hr_sum += h * dur;
                    t.run_hr_n += dur;
                }
            }
            Modality::Tennis => t.tennis_h += dur / 3600.0,
            Modality::Strength => {
                t.strength_sessions += 1.0;
                t.strength_volume += strength_volume.get(&a.id).copied().unwrap_or(0.0);
            }
            Modality::Other => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: i64, day: u32, pace: f64, hr: f64, pts: Vec<[f64; 2]>) -> (Activity, Metrics) {
        let d = NaiveDate::from_ymd_opt(2026, 8, day).unwrap();
        let a = Activity {
            id,
            start_time: d.and_hms_opt(7, 0, 0).unwrap().and_utc().timestamp(),
            modality: Modality::RoadRun,
            distance_m: Some(5000.0),
            moving_s: Some(pace * 5.0),
            avg_hr: Some(hr),
            ..Default::default()
        };
        (a, Metrics { pace_hr: pts, ..Default::default() })
    }

    #[test]
    fn bands_and_hr_at_pace() {
        let (a1, m1) = run(1, 3, 362.0, 179.0, vec![[361.0, 178.0], [362.0, 179.0], [358.0, 180.0], [400.0, 165.0]]);
        let (a2, m2) = run(2, 26, 345.0, 168.0, vec![[359.0, 168.0], [360.0, 167.0], [364.0, 169.0]]);
        let (a3, m3) = run(3, 10, 361.0, 172.0, vec![]); // no steady stretches: uses the average
        let acts = vec![a1, a2, a3];
        let metrics: HashMap<i64, Metrics> = [(1, m1), (2, m2), (3, m3)].into_iter().collect();
        let bands = pace_bands(acts.iter(), &metrics);
        let b6 = bands.iter().find(|b| b.pace == 360.0).unwrap();
        assert_eq!(b6.minutes, 4);
        assert_eq!(b6.activities, 2);
        let h = hr_at_pace(acts.iter(), &metrics, 360.0);
        assert_eq!(h.len(), 3);
        assert_eq!(h[0].activity_id, 1);
        assert!((h[0].hr - 179.0).abs() < 0.01);
        assert_eq!(h[1].minutes, 0);
        assert!((h[2].hr - 168.0).abs() < 0.01);
        assert_eq!(pace_band(367.0), 360.0);
    }

    #[test]
    fn comparability_flags_treadmill_vs_road() {
        let (mut a, _) = run(1, 3, 360.0, 170.0, vec![]);
        let (b, _) = run(2, 4, 360.0, 170.0, vec![]);
        a.modality = Modality::Treadmill;
        let notes = comparability_notes(&[&a, &b], &HashMap::new());
        assert!(notes.iter().any(|n| n.contains("esteira")));
    }
}
