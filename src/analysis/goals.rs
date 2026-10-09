//! Progress of goals (items 17 and 18), computed from the activities.

use super::load::{day_of, week_start};
use crate::format;
use crate::model::Activity;
use crate::planning::{Goal, GoalKind, GoalPeriod};
use chrono::{Datelike, Duration, NaiveDate};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub fraction: f32,
    pub done: bool,
    /// "65 / 100 km"
    pub text: String,
    /// Where the number came from (activity, date...).
    pub detail: String,
}

/// Date range the goal looks at today.
pub fn period_range(g: &Goal, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    match g.params.period {
        GoalPeriod::Week => {
            let s = week_start(today);
            (s, s + Duration::days(6))
        }
        GoalPeriod::Month => {
            let s = today.with_day(1).unwrap_or(today);
            let e = s.checked_add_months(chrono::Months::new(1)).map(|d| d - Duration::days(1)).unwrap_or(today);
            (s, e)
        }
        GoalPeriod::Range => (
            g.start_date.unwrap_or(NaiveDate::from_ymd_opt(1990, 1, 1).unwrap()),
            g.end_date.unwrap_or(NaiveDate::from_ymd_opt(2999, 12, 31).unwrap()),
        ),
    }
}

/// `best_efforts`: activity id → [(distance, time)].
pub fn progress(g: &Goal, acts: &[Activity], best_efforts: &HashMap<i64, Vec<(f64, f64)>>, today: NaiveDate) -> Progress {
    let p = &g.params;
    // performance-type goals look at the whole goal range, not at the calendar period
    let range = match g.kind {
        GoalKind::Performance | GoalKind::HeartRate | GoalKind::MaxDistance => (
            g.start_date.unwrap_or(NaiveDate::from_ymd_opt(1990, 1, 1).unwrap()),
            g.end_date.unwrap_or(NaiveDate::from_ymd_opt(2999, 12, 31).unwrap()),
        ),
        _ => period_range(g, today),
    };
    let in_range: Vec<&Activity> =
        acts.iter().filter(|a| p.matches(a.modality)).filter(|a| (range.0..=range.1).contains(&day_of(a))).collect();
    let frac = |cur: f64, target: f64| if target > 0.0 { (cur / target) as f32 } else { 0.0 };
    match g.kind {
        GoalKind::Distance => {
            let target = p.distance_m.unwrap_or(0.0) / 1000.0;
            let cur: f64 = in_range.iter().filter_map(|a| a.distance()).sum::<f64>() / 1000.0;
            Progress {
                fraction: frac(cur, target),
                done: target > 0.0 && cur >= target,
                text: format!("{} / {} km", format::num(cur, 1), format::num_trim(target, 1)),
                detail: trf!("{} atividade(s) no período", "{} activity(ies) in the period", in_range.len()),
            }
        }
        GoalKind::Frequency => {
            let target = p.count.unwrap_or(0.0);
            // this week's count; detail shows the average of the last 4 weeks
            let ws = week_start(today);
            let cur = acts.iter().filter(|a| p.matches(a.modality) && (ws..=today).contains(&day_of(a))).count() as f64;
            let last4 = acts
                .iter()
                .filter(|a| p.matches(a.modality))
                .filter(|a| (ws - Duration::days(28)..ws).contains(&day_of(a)))
                .count() as f64
                / 4.0;
            Progress {
                fraction: frac(cur, target),
                done: target > 0.0 && cur >= target,
                text: trf!("{} / {} nesta semana", "{} / {} this week", cur, format::num_trim(target, 1)),
                detail: trf!(
                    "média das 4 semanas anteriores: {} por semana",
                    "average of the previous 4 weeks: {} per week",
                    format::num(last4, 1)
                ),
            }
        }
        GoalKind::Hours => {
            let target = p.hours.unwrap_or(0.0);
            let cur: f64 = in_range.iter().filter_map(|a| a.duration()).sum::<f64>() / 3600.0;
            Progress {
                fraction: frac(cur, target),
                done: target > 0.0 && cur >= target,
                text: format!("{} / {}", format::hours_short(cur * 3600.0), format::hours_short(target * 3600.0)),
                detail: trf!("{} sessão(ões) no período", "{} session(s) in the period", in_range.len()),
            }
        }
        GoalKind::MaxDistance => {
            let target = p.distance_m.unwrap_or(0.0);
            let best = in_range.iter().filter_map(|a| a.distance().map(|d| (d, *a))).max_by(|x, y| x.0.total_cmp(&y.0));
            let cur = best.map(|b| b.0).unwrap_or(0.0);
            Progress {
                fraction: frac(cur, target),
                done: target > 0.0 && cur >= target * 0.995,
                text: format!("{} / {} km", format::num(cur / 1000.0, 2), format::num_trim(target / 1000.0, 2)),
                detail: best
                    .map(|(_, a)| trf!("maior: {}", "longest: {}", format::date(day_of(a))))
                    .unwrap_or_default(),
            }
        }
        GoalKind::Performance => {
            let dist = p.distance_m.unwrap_or(0.0);
            let target = p.time_s.unwrap_or(0.0);
            let best = in_range
                .iter()
                .filter_map(|a| {
                    let list = best_efforts.get(&a.id)?;
                    let t = list.iter().find(|(d, _)| (d - dist).abs() < 1.0).map(|(_, t)| *t).or_else(|| {
                        // distance not in the records list: whole activity if close enough
                        let ad = a.distance()?;
                        ((ad - dist).abs() <= dist * 0.02).then(|| a.duration()).flatten()
                    })?;
                    Some((t, *a))
                })
                .min_by(|x, y| x.0.total_cmp(&y.0));
            match best {
                Some((t, a)) => Progress {
                    fraction: if t > 0.0 { (target / t).min(1.0) as f32 } else { 0.0 },
                    done: t <= target,
                    text: trf!("melhor: {} (meta {})", "best: {} (goal {})", format::duration(t), format::duration(target)),
                    detail: format::date(day_of(a)),
                },
                None => Progress {
                    fraction: 0.0,
                    done: false,
                    text: trf!("meta {}", "goal {}", format::duration(target)),
                    detail: tr!("nenhuma corrida com essa distância ainda", "no run with this distance yet").into(),
                },
            }
        }
        GoalKind::HeartRate => {
            let dist = p.distance_m.unwrap_or(0.0);
            let pace = p.pace.unwrap_or(0.0);
            let target = p.hr.unwrap_or(0.0);
            let best = in_range
                .iter()
                .filter(|a| a.distance().is_some_and(|d| d >= dist * 0.97))
                .filter(|a| a.pace_s_per_km().is_some_and(|pc| pc <= pace + 5.0))
                .filter_map(|a| a.avg_hr.map(|h| (h, *a)))
                .min_by(|x, y| x.0.total_cmp(&y.0));
            match best {
                Some((h, a)) => Progress {
                    fraction: if h > 0.0 { (target / h).min(1.0) as f32 } else { 0.0 },
                    done: h < target,
                    text: trf!("melhor: {} (meta < {})", "best: {} (goal < {})", format::bpm(Some(h)), format::bpm(Some(target))),
                    detail: trf!(
                        "{} · pace {}",
                        "{} · pace {}",
                        format::date(day_of(a)),
                        format::opt_pace(a.pace_s_per_km())
                    ),
                },
                None => Progress {
                    fraction: 0.0,
                    done: false,
                    text: trf!("meta < {}", "goal < {}", format::bpm(Some(target))),
                    detail: tr!("nenhuma corrida nessas condições ainda", "no run in these conditions yet").into(),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Modality;
    use crate::planning::GoalParams;

    fn act(id: i64, date: &str, m: Modality, km: f64, secs: f64, hr: f64) -> Activity {
        let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
        Activity {
            id,
            start_time: d.and_hms_opt(7, 0, 0).unwrap().and_utc().timestamp(),
            modality: m,
            distance_m: Some(km * 1000.0),
            moving_s: Some(secs),
            avg_hr: Some(hr),
            ..Default::default()
        }
    }

    #[test]
    fn distance_goal_in_month() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        let acts = vec![
            act(1, "2026-08-03", Modality::RoadRun, 40.0, 0.0, 0.0),
            act(2, "2026-08-20", Modality::Treadmill, 25.0, 0.0, 0.0),
            act(3, "2026-07-20", Modality::RoadRun, 50.0, 0.0, 0.0),
            act(4, "2026-08-21", Modality::Tennis, 0.0, 3600.0, 0.0),
        ];
        let g = Goal {
            kind: GoalKind::Distance,
            params: GoalParams { distance_m: Some(100_000.0), period: GoalPeriod::Month, ..Default::default() },
            ..Default::default()
        };
        let p = progress(&g, &acts, &HashMap::new(), today);
        assert_eq!(p.text, "65,0 / 100 km");
        assert!((p.fraction - 0.65).abs() < 1e-6);
        assert!(!p.done);

        let tennis = Goal {
            kind: GoalKind::Hours,
            params: GoalParams { hours: Some(4.0), modality: Some("tennis".into()), ..Default::default() },
            ..Default::default()
        };
        let p = progress(&tennis, &acts, &HashMap::new(), today);
        assert!((p.fraction - 0.25).abs() < 1e-6);
    }

    #[test]
    fn performance_and_heart_rate_goals() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        let acts = vec![
            act(1, "2026-08-09", Modality::RoadRun, 5.03, 1776.0, 174.0),
            act(2, "2026-08-26", Modality::RoadRun, 5.01, 1729.0, 168.0),
        ];
        let be: HashMap<i64, Vec<(f64, f64)>> = [(1, vec![(5000.0, 1765.0)]), (2, vec![(5000.0, 1725.0)])].into_iter().collect();
        let g = Goal {
            kind: GoalKind::Performance,
            params: GoalParams { distance_m: Some(5000.0), time_s: Some(28.0 * 60.0), ..Default::default() },
            ..Default::default()
        };
        let p = progress(&g, &acts, &be, today);
        assert!(!p.done);
        assert!(p.text.contains("28:45"), "{}", p.text);

        let hr = Goal {
            kind: GoalKind::HeartRate,
            params: GoalParams { distance_m: Some(5000.0), pace: Some(360.0), hr: Some(170.0), ..Default::default() },
            ..Default::default()
        };
        let p = progress(&hr, &acts, &be, today);
        assert!(p.done, "{p:?}");
    }
}
