//! Training load over days (items 21, 22, 27): daily/weekly load, acute (7 d)
//! and chronic (28 d) load, simple warnings and a recovery estimate.
//! Tennis and strength count like any other session: a hard tennis match is
//! not a rest day. No medical diagnosis is made.

use super::metrics::Metrics;
use crate::model::{Activity, Modality};
use chrono::{Datelike, Duration, NaiveDate};
use std::collections::{BTreeMap, HashMap};

pub fn day_of(a: &Activity) -> NaiveDate {
    a.local_start().date()
}

pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

/// Load of an activity: computed metrics, or the stored column.
pub fn activity_load(a: &Activity, m: Option<&Metrics>) -> f64 {
    m.map(|m| m.load).or(a.load).unwrap_or(0.0)
}

/// Sum of load per day.
pub fn daily_load(acts: &[Activity], metrics: &HashMap<i64, Metrics>) -> BTreeMap<NaiveDate, f64> {
    let mut out = BTreeMap::new();
    for a in acts {
        *out.entry(day_of(a)).or_insert(0.0) += activity_load(a, metrics.get(&a.id));
    }
    out
}

/// Load in the `days` days ending on `end` (inclusive).
pub fn window_load(daily: &BTreeMap<NaiveDate, f64>, end: NaiveDate, days: i64) -> f64 {
    daily.range(end - Duration::days(days - 1)..=end).map(|(_, v)| v).sum()
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadStatus {
    pub today: f64,
    pub acute7: f64,
    pub chronic28: f64,
    /// Acute load / weekly average of the last 28 days.
    pub ratio: Option<f64>,
    pub this_week: f64,
    pub last_week: f64,
}

pub fn status(daily: &BTreeMap<NaiveDate, f64>, today: NaiveDate) -> LoadStatus {
    let acute7 = window_load(daily, today, 7);
    let chronic28 = window_load(daily, today, 28);
    let weekly_avg = chronic28 / 4.0;
    let ws = week_start(today);
    LoadStatus {
        today: daily.get(&today).copied().unwrap_or(0.0),
        acute7,
        chronic28,
        ratio: (weekly_avg > 1.0).then(|| acute7 / weekly_avg),
        this_week: daily.range(ws..=today).map(|(_, v)| v).sum(),
        last_week: daily.range(ws - Duration::days(7)..ws).map(|(_, v)| v).sum(),
    }
}

/// Simple warnings with the data that triggered them.
pub fn warnings(acts: &[Activity], metrics: &HashMap<i64, Metrics>, today: NaiveDate) -> Vec<String> {
    let mut out = Vec::new();
    let daily = daily_load(acts, metrics);
    let s = status(&daily, today);
    // weeks before the current one, to compare volume
    let ws = week_start(today);
    let prev: Vec<f64> = (1..=4)
        .map(|k| {
            let a = ws - Duration::days(7 * k);
            daily.range(a..a + Duration::days(7)).map(|(_, v)| v).sum::<f64>()
        })
        .collect();
    let has_history = prev.iter().filter(|v| **v > 0.0).count() >= 2;
    let avg_prev = prev.iter().sum::<f64>() / 4.0;
    // compare the days elapsed this week with the same share of a normal week
    let elapsed = (today - ws).num_days() + 1;
    if has_history
        && avg_prev > 0.0
        && s.this_week > avg_prev * 1.3
        && s.this_week > avg_prev * elapsed as f64 / 7.0 * 1.3
    {
        out.push(trf!(
            "Volume desta semana está significativamente acima da sua média recente (carga {} contra média de {} nas 4 semanas anteriores).",
            "This week's volume is significantly above your recent average (load {} vs an average of {} in the previous 4 weeks).",
            s.this_week.round(),
            avg_prev.round()
        ));
    }
    // two hard sessions on consecutive days (last 7 days)
    let mut hard_days: Vec<(NaiveDate, String)> = acts
        .iter()
        .filter(|a| day_of(a) > today - Duration::days(7) && day_of(a) <= today)
        .filter(|a| metrics.get(&a.id).is_some_and(|m| m.is_hard(a)))
        .map(|a| (day_of(a), a.modality.label().to_string()))
        .collect();
    hard_days.sort();
    hard_days.dedup_by_key(|(d, _)| *d);
    for w in hard_days.windows(2) {
        if (w[1].0 - w[0].0).num_days() == 1 {
            out.push(trf!(
                "Você realizou dois treinos intensos em dias consecutivos ({} em {} e {} em {}).",
                "You did two hard sessions on consecutive days ({} on {} and {} on {}).",
                w[0].1,
                crate::format::date_short(w[0].0),
                w[1].1,
                crate::format::date_short(w[1].0)
            ));
        }
    }
    let streak = consecutive_days(acts, today);
    if streak >= 6 {
        out.push(trf!(
            "{streak} dias seguidos treinando, sem descanso.",
            "{streak} days in a row training, without rest."
        ));
    }
    out
}

/// Days in a row (ending today or yesterday) with at least one activity.
pub fn consecutive_days(acts: &[Activity], today: NaiveDate) -> i64 {
    let days: std::collections::BTreeSet<NaiveDate> = acts.iter().map(day_of).collect();
    let mut d = if days.contains(&today) {
        today
    } else {
        today - Duration::days(1)
    };
    let mut n = 0;
    while days.contains(&d) {
        n += 1;
        d -= Duration::days(1);
    }
    n
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    Recovered,
    Moderate,
    High,
}

impl Recovery {
    pub fn label(self) -> &'static str {
        match self {
            Recovery::Recovered => tr!("Recuperado", "Recovered"),
            Recovery::Moderate => tr!("Carga moderada", "Moderate load"),
            Recovery::High => tr!("Carga alta", "High load"),
        }
    }
}

/// Recovery estimate from recent load, streak and the last 48 h.
/// Returns the class and the reasons, to show next to it.
pub fn recovery(acts: &[Activity], metrics: &HashMap<i64, Metrics>, today: NaiveDate) -> (Recovery, Vec<String>) {
    let daily = daily_load(acts, metrics);
    let s = status(&daily, today);
    let last48 = window_load(&daily, today, 2);
    let streak = consecutive_days(acts, today);
    let weekly_avg = s.chronic28 / 4.0;
    let daily_avg = (s.chronic28 / 28.0).max(1.0);
    let hard48 = acts
        .iter()
        .filter(|a| (today - day_of(a)).num_days() < 2 && day_of(a) <= today)
        .filter(|a| metrics.get(&a.id).is_some_and(|m| m.is_hard(a)))
        .count();
    let mut points = 0;
    let mut why = Vec::new();
    // the 7 d / 28 d ratio only means something with some history before this week
    let history_weeks = (1..4)
        .filter(|k| {
            let end = today - Duration::days(7 * k);
            window_load(&daily, end, 7) > 0.0
        })
        .count();
    if history_weeks < 2 {
        why.push(
            tr!(
                "pouco histórico nas últimas 4 semanas para comparar a carga",
                "little history in the last 4 weeks to compare the load"
            )
            .to_string(),
        );
    }
    if let Some(r) = s.ratio.filter(|_| history_weeks >= 2) {
        why.push(trf!(
            "carga dos últimos 7 dias: {} (média semanal das últimas 4 semanas: {})",
            "load of the last 7 days: {} (weekly average of the last 4 weeks: {})",
            s.acute7.round(),
            weekly_avg.round()
        ));
        if r > 1.5 {
            points += 2;
        } else if r > 1.2 {
            points += 1;
        }
    }
    if last48 > daily_avg * 3.0 && last48 > 60.0 {
        points += 1;
        why.push(trf!(
            "carga das últimas 48 h: {}",
            "load of the last 48 h: {}",
            last48.round()
        ));
    }
    if hard48 > 0 {
        points += 1;
        why.push(trf!(
            "{hard48} treino(s) intenso(s) nas últimas 48 h",
            "{hard48} hard session(s) in the last 48 h"
        ));
    }
    if streak >= 5 {
        points += 1;
        why.push(trf!(
            "{streak} dias seguidos treinando",
            "{streak} days in a row training"
        ));
    }
    let class = match points {
        0 | 1 => Recovery::Recovered,
        2 => Recovery::Moderate,
        _ => Recovery::High,
    };
    (class, why)
}

/// Weekly summary (item 27).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeekSummary {
    pub start: Option<NaiveDate>,
    pub run_km: f64,
    pub run_count: usize,
    pub tennis_s: f64,
    pub strength_sessions: usize,
    pub total_s: f64,
    pub longest_run_km: f64,
    /// Average HR of easy runs (pace slower than the average pace of the week).
    pub easy_hr: Option<f64>,
    pub load: f64,
    pub activities: usize,
}

pub fn week_summary(acts: &[Activity], metrics: &HashMap<i64, Metrics>, start: NaiveDate) -> WeekSummary {
    let end = start + Duration::days(7);
    let week: Vec<&Activity> = acts.iter().filter(|a| (start..end).contains(&day_of(a))).collect();
    let mut s = WeekSummary {
        start: Some(start),
        activities: week.len(),
        ..Default::default()
    };
    let mut runs = Vec::new();
    for a in &week {
        s.total_s += a.duration().unwrap_or(0.0);
        s.load += activity_load(a, metrics.get(&a.id));
        match a.modality {
            Modality::RoadRun | Modality::Treadmill => {
                let km = a.distance().unwrap_or(0.0) / 1000.0;
                s.run_km += km;
                s.run_count += 1;
                s.longest_run_km = s.longest_run_km.max(km);
                runs.push(*a);
            }
            Modality::Tennis => s.tennis_s += a.duration().unwrap_or(0.0),
            Modality::Strength => s.strength_sessions += 1,
            Modality::Other => {}
        }
    }
    let paces: Vec<f64> = runs.iter().filter_map(|a| a.pace_s_per_km()).collect();
    if !paces.is_empty() {
        let avg = paces.iter().sum::<f64>() / paces.len() as f64;
        let easy: Vec<f64> = runs
            .iter()
            .filter(|a| a.pace_s_per_km().is_some_and(|p| p >= avg - 1.0))
            .filter_map(|a| a.avg_hr)
            .collect();
        if !easy.is_empty() {
            s.easy_hr = Some(easy.iter().sum::<f64>() / easy.len() as f64);
        }
    }
    s
}

/// Load class of a week compared with the previous 4 weeks.
pub fn load_class(week_load: f64, prev_avg: f64) -> &'static str {
    if prev_avg <= 0.0 {
        return if week_load > 0.0 {
            tr!("Moderada", "Moderate")
        } else {
            tr!("Baixa", "Low")
        };
    }
    let r = week_load / prev_avg;
    if r < 0.8 {
        tr!("Baixa", "Low")
    } else if r <= 1.3 {
        tr!("Moderada", "Moderate")
    } else {
        tr!("Alta", "High")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::metrics::Metrics;

    fn act(id: i64, day: NaiveDate, m: Modality, load: f64, hard: bool) -> (Activity, Metrics) {
        let start = day.and_hms_opt(8, 0, 0).unwrap().and_utc().timestamp();
        let a = Activity {
            id,
            start_time: start,
            modality: m,
            elapsed_s: Some(3600.0),
            distance_m: if m.is_run() { Some(8000.0) } else { None },
            ..Default::default()
        };
        let mt = Metrics {
            load,
            hard_share: if hard { 0.5 } else { 0.0 },
            ..Default::default()
        };
        (a, mt)
    }

    fn build(list: Vec<(Activity, Metrics)>) -> (Vec<Activity>, HashMap<i64, Metrics>) {
        let mut acts = Vec::new();
        let mut map = HashMap::new();
        for (a, m) in list {
            map.insert(a.id, m);
            acts.push(a);
        }
        (acts, map)
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn tennis_counts_as_load_and_hard_consecutive_days_warn() {
        let today = d(2026, 10, 9);
        let (acts, m) = build(vec![
            act(1, d(2026, 10, 7), Modality::RoadRun, 90.0, true),
            act(2, d(2026, 10, 8), Modality::Tennis, 150.0, true),
        ]);
        let daily = daily_load(&acts, &m);
        assert_eq!(window_load(&daily, today, 7), 240.0);
        let w = warnings(&acts, &m, today);
        assert!(w.iter().any(|s| s.contains("consecutivos")), "{w:?}");
        let (r, why) = recovery(&acts, &m, today);
        assert_ne!(r, Recovery::Recovered);
        assert!(!why.is_empty());
    }

    #[test]
    fn volume_warning_needs_history() {
        let today = d(2026, 10, 9); // Friday
        let mut list = Vec::new();
        let mut id = 0;
        for w in 1..=4 {
            for k in 0..3 {
                id += 1;
                list.push(act(
                    id,
                    week_start(today) - Duration::days(7 * w) + Duration::days(k * 2),
                    Modality::RoadRun,
                    50.0,
                    false,
                ));
            }
        }
        for k in 0..4 {
            id += 1;
            list.push(act(
                id,
                week_start(today) + Duration::days(k),
                Modality::RoadRun,
                80.0,
                false,
            ));
        }
        let (acts, m) = build(list);
        let w = warnings(&acts, &m, today);
        assert!(w.iter().any(|s| s.contains("acima da sua média")), "{w:?}");
    }

    #[test]
    fn weekly_summary() {
        let start = d(2026, 10, 5);
        let (acts, m) = build(vec![
            act(1, d(2026, 10, 5), Modality::RoadRun, 50.0, false),
            act(2, d(2026, 10, 6), Modality::Tennis, 70.0, false),
            act(3, d(2026, 10, 7), Modality::Strength, 30.0, false),
            act(4, d(2026, 10, 12), Modality::RoadRun, 50.0, false),
        ]);
        let s = week_summary(&acts, &m, start);
        assert_eq!(s.run_count, 1);
        assert_eq!(s.run_km, 8.0);
        assert_eq!(s.tennis_s, 3600.0);
        assert_eq!(s.strength_sessions, 1);
        assert_eq!(s.total_s, 3.0 * 3600.0);
        assert_eq!(s.load, 150.0);
    }

    #[test]
    fn streak() {
        let today = d(2026, 10, 9);
        let (acts, _) = build(
            (0..7)
                .map(|i| act(i, today - Duration::days(i), Modality::Tennis, 10.0, false))
                .collect(),
        );
        assert_eq!(consecutive_days(&acts, today), 7);
    }
}
