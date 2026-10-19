//! What the recent runs say about the runner's current fitness.

use super::{Goal, week_start};
use crate::analysis::metrics::Metrics;
use crate::model::Activity;
use crate::settings::Zones;
use chrono::{Duration, NaiveDate};
use std::collections::HashMap;

/// How many days of runs feed the fitness model.
pub const WINDOW_DAYS: i64 = 42;
/// Typical rise of heart rate per m/s of speed when the data cannot tell
/// (≈ 7 bpm per km/h).
const PRIOR_SLOPE: f64 = 25.0;
/// Pace used when there are no runs yet (beginner easy pace, s/km).
pub const BEGINNER_EASY_PACE: f64 = 480.0;

/// Steady-state heart rate as a function of speed: hr = a + b × speed (m/s).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HrModel {
    pub a: f64,
    pub b: f64,
    pub points: usize,
}

impl HrModel {
    pub fn hr_at_pace(&self, pace: f64) -> f64 {
        self.a + self.b * (1000.0 / pace)
    }

    /// Pace (s/km) at which the model reaches `hr`. None if unrealistic.
    pub fn pace_at_hr(&self, hr: f64) -> Option<f64> {
        let speed = (hr - self.a) / self.b;
        (speed > 1.2 && speed < 7.0).then(|| 1000.0 / speed)
    }
}

/// Least squares fit; falls back to the prior slope when speeds barely vary
/// (e.g. only treadmill runs at the same speed).
pub fn fit_model(points: &[(f64, f64)]) -> Option<HrModel> {
    if points.len() < 3 {
        return None;
    }
    let n = points.len() as f64;
    let mx = points.iter().map(|p| p.0).sum::<f64>() / n;
    let my = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let sxy: f64 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let spread = (sxx / n).sqrt();
    let mut b = if spread >= 0.15 { sxy / sxx } else { PRIOR_SLOPE };
    // physiologically HR rises with speed; keep the slope sane
    if !(8.0..=60.0).contains(&b) {
        b = PRIOR_SLOPE;
    }
    Some(HrModel {
        a: my - b * mx,
        b,
        points: points.len(),
    })
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fitness {
    pub runs_28d: usize,
    /// Average running minutes per week over the last 3 full weeks.
    pub weekly_minutes: f64,
    pub weekly_km: f64,
    /// Running minutes of the last complete week.
    pub last_week_minutes: f64,
    /// Longest run of the last 28 days, meters and seconds.
    pub longest_m: f64,
    pub longest_s: f64,
    pub model: Option<HrModel>,
    /// Pace at the top of zone 2 (the easy-run limit), s/km.
    pub easy_pace: f64,
    pub easy_hr_cap: f64,
    /// Riegel prediction of the goal distance from the best recent effort.
    pub predicted_time: Option<f64>,
    /// Heart rate at the goal pace, according to the model.
    pub hr_at_goal_pace: Option<f64>,
    /// bpm per week (negative = improving), from the HR-at-goal-pace history.
    pub hr_trend_per_week: Option<f64>,
    /// (date, estimated HR at the goal pace) per run, for the progress chart.
    pub hr_history: Vec<(NaiveDate, f64)>,
}

pub struct Inputs<'a> {
    /// Runs only.
    pub runs: &'a [Activity],
    pub metrics: &'a HashMap<i64, Metrics>,
    /// activity id → [(distance, time)].
    pub best_efforts: &'a HashMap<i64, Vec<(f64, f64)>>,
    pub zones: &'a Zones,
    pub today: NaiveDate,
}

pub fn day(a: &Activity) -> NaiveDate {
    a.local_start().date()
}

/// (speed m/s, hr) points of one run: steady stretches, else the run average.
fn run_points(a: &Activity, m: Option<&Metrics>) -> Vec<(f64, f64)> {
    let steady: Vec<(f64, f64)> = m
        .map(|m| {
            m.pace_hr
                .iter()
                .filter(|p| p[0] > 0.0)
                .map(|p| (1000.0 / p[0], p[1]))
                .collect()
        })
        .unwrap_or_default();
    if steady.len() >= 3 {
        return steady;
    }
    match (a.pace_s_per_km(), a.avg_hr) {
        (Some(p), Some(h)) if (150.0..1200.0).contains(&p) => vec![(1000.0 / p, h)],
        _ => Vec::new(),
    }
}

pub fn assess(inp: &Inputs, goal: &Goal) -> Fitness {
    let today = inp.today;
    let mut f = Fitness::default();
    let recent: Vec<&Activity> = inp
        .runs
        .iter()
        .filter(|a| day(a) > today - Duration::days(WINDOW_DAYS) && day(a) <= today)
        .collect();
    f.runs_28d = inp
        .runs
        .iter()
        .filter(|a| day(a) > today - Duration::days(28) && day(a) <= today)
        .count();

    // volume: last 3 complete weeks
    let ws = week_start(today);
    let (mut mins, mut km) = (0.0, 0.0);
    for a in inp
        .runs
        .iter()
        .filter(|a| (ws - Duration::days(21)..ws).contains(&day(a)))
    {
        mins += a.duration().unwrap_or(0.0) / 60.0;
        km += a.distance().unwrap_or(0.0) / 1000.0;
    }
    f.weekly_minutes = mins / 3.0;
    f.last_week_minutes = inp
        .runs
        .iter()
        .filter(|a| (ws - Duration::days(7)..ws).contains(&day(a)))
        .map(|a| a.duration().unwrap_or(0.0) / 60.0)
        .sum();
    f.weekly_km = km / 3.0;

    for a in inp
        .runs
        .iter()
        .filter(|a| day(a) > today - Duration::days(28) && day(a) <= today)
    {
        let d = a.distance().unwrap_or(0.0);
        if d > f.longest_m {
            f.longest_m = d;
            f.longest_s = a.duration().unwrap_or(0.0);
        }
    }

    // heart rate × speed model from the window (all runs if the window is empty)
    let source: Vec<&Activity> = if recent.is_empty() {
        inp.runs.iter().collect()
    } else {
        recent.clone()
    };
    let points: Vec<(f64, f64)> = source
        .iter()
        .flat_map(|a| run_points(a, inp.metrics.get(&a.id)))
        .collect();
    f.model = fit_model(&points);
    f.easy_hr_cap = inp.zones.range(2).1;
    f.easy_pace = f
        .model
        .and_then(|m| m.pace_at_hr(f.easy_hr_cap))
        .or_else(|| {
            // no model: recent average pace plus a margin
            let paces: Vec<f64> = source.iter().filter_map(|a| a.pace_s_per_km()).collect();
            (!paces.is_empty()).then(|| paces.iter().sum::<f64>() / paces.len() as f64 + 20.0)
        })
        .unwrap_or(BEGINNER_EASY_PACE)
        .clamp(270.0, 660.0);

    // prediction for the goal distance (Riegel, exponent 1.06) from the best
    // effort of the window, using efforts of at least 1 km
    let mut best: Option<f64> = None;
    for a in &source {
        if let Some(list) = inp.best_efforts.get(&a.id) {
            for &(d, t) in list.iter().filter(|(d, _)| *d >= 1000.0) {
                let pred = t * (goal.distance_m / d).powf(1.06);
                best = Some(best.map_or(pred, |b: f64| b.min(pred)));
            }
        }
        // a whole run longer than the efforts list covers
        if let (Some(d), Some(t)) = (a.distance(), a.duration())
            && d >= 1000.0
        {
            let pred = t * (goal.distance_m / d).powf(1.06);
            best = Some(best.map_or(pred, |b: f64| b.min(pred)));
        }
    }
    f.predicted_time = best;

    // heart rate at the goal pace: now and its history
    let ref_pace = goal.goal_pace().unwrap_or(f.easy_pace);
    if let Some(m) = f.model {
        f.hr_at_goal_pace = Some(m.hr_at_pace(ref_pace));
        let ref_speed = 1000.0 / ref_pace;
        for a in inp
            .runs
            .iter()
            .filter(|a| day(a) > today - Duration::days(70) && day(a) <= today)
        {
            let pts = run_points(a, inp.metrics.get(&a.id));
            if pts.is_empty() {
                continue;
            }
            // shift each point to the reference speed along the model slope
            let adj: f64 = pts.iter().map(|(s, h)| h - m.b * (s - ref_speed)).sum::<f64>() / pts.len() as f64;
            f.hr_history.push((day(a), adj));
        }
        f.hr_history.sort_by_key(|x| x.0);
        if f.hr_history.len() >= 3 {
            let t0 = f.hr_history[0].0;
            let xs: Vec<(f64, f64)> = f
                .hr_history
                .iter()
                .map(|(d, h)| ((*d - t0).num_days() as f64 / 7.0, *h))
                .collect();
            let n = xs.len() as f64;
            let mx = xs.iter().map(|p| p.0).sum::<f64>() / n;
            let my = xs.iter().map(|p| p.1).sum::<f64>() / n;
            let sxx: f64 = xs.iter().map(|p| (p.0 - mx).powi(2)).sum();
            if sxx > 0.5 {
                f.hr_trend_per_week = Some(xs.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / sxx);
            }
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Modality;
    use crate::settings::Settings;

    fn run(id: i64, date: NaiveDate, km: f64, pace: f64, hr: f64) -> Activity {
        Activity {
            id,
            start_time: date.and_hms_opt(7, 0, 0).unwrap().and_utc().timestamp(),
            modality: Modality::RoadRun,
            distance_m: Some(km * 1000.0),
            moving_s: Some(km * pace),
            avg_hr: Some(hr),
            ..Default::default()
        }
    }

    #[test]
    fn model_fit_and_inverse() {
        // hr = 60 + 30 × speed
        let pts: Vec<(f64, f64)> = [2.5, 2.8, 3.0, 3.3].iter().map(|s| (*s, 60.0 + 30.0 * s)).collect();
        let m = fit_model(&pts).unwrap();
        assert!((m.b - 30.0).abs() < 1e-6);
        assert!((m.hr_at_pace(400.0) - 135.0).abs() < 1e-6);
        assert!((m.pace_at_hr(135.0).unwrap() - 400.0).abs() < 1e-6);
        // no spread: prior slope, same mean
        let flat = vec![(2.78, 165.0), (2.78, 167.0), (2.78, 166.0)];
        let m = fit_model(&flat).unwrap();
        assert_eq!(m.b, PRIOR_SLOPE);
        assert!((m.hr_at_pace(360.0) - 166.0).abs() < 0.1);
    }

    #[test]
    fn assess_recent_runs() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(); // Monday
        let runs: Vec<Activity> = (0..9)
            .map(|i| {
                let d = today - Duration::days(3 + i * 3);
                // getting fitter: same pace, HR drops over time
                run(
                    i,
                    d,
                    5.0,
                    380.0 - (i % 3) as f64 * 20.0,
                    160.0 + i as f64 + (i % 3) as f64 * 8.0,
                )
            })
            .collect();
        let zones = Settings {
            hr_max: Some(190.0),
            ..Default::default()
        }
        .zones(None);
        let goal = Goal {
            distance_m: 5000.0,
            target_time_s: Some(1800.0),
            ..Default::default()
        };
        let f = assess(
            &Inputs {
                runs: &runs,
                metrics: &HashMap::new(),
                best_efforts: &HashMap::new(),
                zones: &zones,
                today,
            },
            &goal,
        );
        assert!(f.weekly_minutes > 50.0, "{}", f.weekly_minutes);
        assert!((f.longest_m - 5000.0).abs() < 1e-6);
        assert!(f.model.is_some());
        assert!(f.easy_pace > 360.0, "{}", f.easy_pace);
        assert!(f.predicted_time.unwrap() < 1900.0);
        assert!(f.hr_trend_per_week.unwrap() < 0.0, "{:?}", f.hr_trend_per_week);
    }

    #[test]
    fn beginner_without_runs() {
        let zones = Settings::default().zones(None);
        let today = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let f = assess(
            &Inputs {
                runs: &[],
                metrics: &HashMap::new(),
                best_efforts: &HashMap::new(),
                zones: &zones,
                today,
            },
            &Goal::default(),
        );
        assert_eq!(f.easy_pace, BEGINNER_EASY_PACE);
        assert!(f.model.is_none());
        assert_eq!(f.weekly_minutes, 0.0);
    }
}
