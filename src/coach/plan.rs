//! Goal projection and schedule generation.
//!
//! The schedule is a pure function of (goal, fitness, adjustments, today):
//! calling it again after a new run or a goal change rebuilds the future
//! consistently. Rules are deliberately conservative for a new runner:
//! running time grows at most 8% a week, every 4th week is lighter, the long
//! run grows about 10% a week up to the goal distance, and harder sessions
//! only start after a few weeks of easy running.

use super::adapt::Adjust;
use super::fitness::{Fitness, day};
use super::{Goal, Phase, Session, SessionKind, Step, Surface, week_start};
use crate::format;
use crate::model::Activity;
use chrono::{Duration, NaiveDate};
use std::collections::HashMap;

/// Longest plan the coach builds ahead.
pub const MAX_WEEKS: u32 = 26;
/// Expected improvement of race time per week for a new runner.
const TIME_GAIN_PER_WEEK: f64 = 0.008;
/// Expected drop of heart rate at a fixed pace per week when the data has no trend yet.
const HR_GAIN_PER_WEEK: f64 = 0.4;

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub label: String,
    /// "now" and "goal" values, already formatted.
    pub now: String,
    pub goal: String,
    pub fraction: f32,
    pub weeks: f64,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    pub weeks: u32,
    pub date: NaiveDate,
    pub parts: Vec<Part>,
    /// Weeks available until the goal date, when there is one.
    pub weeks_available: Option<u32>,
}

/// First run since the goal started that meets every part of the goal.
pub fn goal_reached(
    goal: &Goal,
    runs: &[Activity],
    best_efforts: &HashMap<i64, Vec<(f64, f64)>>,
) -> Option<(NaiveDate, i64)> {
    let mut list: Vec<&Activity> = runs.iter().filter(|a| day(a) >= goal.start_date).collect();
    list.sort_by_key(|a| a.start_time);
    list.into_iter()
        .find(|a| {
            let dist = a.distance().unwrap_or(0.0);
            if dist < goal.distance_m * 0.98 {
                return false;
            }
            if let Some(t) = goal.target_time_s {
                let whole = a.duration().unwrap_or(f64::MAX) * (goal.distance_m / dist);
                let effort = best_efforts
                    .get(&a.id)
                    .and_then(|l| l.iter().find(|(d, _)| (d - goal.distance_m).abs() < 1.0).map(|e| e.1));
                if whole.min(effort.unwrap_or(f64::MAX)) > t {
                    return false;
                }
            }
            if let Some(h) = goal.target_hr {
                if a.avg_hr.is_none_or(|x| x > h) {
                    return false;
                }
                if let (Some(p), Some(gp)) = (a.pace_s_per_km(), goal.goal_pace())
                    && p > gp + 5.0
                {
                    return false;
                }
            }
            true
        })
        .map(|a| (day(a), a.id))
}

/// Weeks for the long run to grow from `from_m` to `to_m` (10% a week, every 4th week held).
fn weeks_to_distance(from_m: f64, to_m: f64) -> f64 {
    let mut d = from_m.max(1500.0);
    let mut w = 0.0;
    while d < to_m * 0.98 && w < 200.0 {
        w += 1.0;
        if (w as i64) % 4 != 0 {
            d *= 1.10;
        }
    }
    w
}

pub fn project(goal: &Goal, f: &Fitness, today: NaiveDate) -> Projection {
    let mut parts = Vec::new();
    let km = goal.distance_m / 1000.0;
    // distance: what the longest recent run says
    let dist_weeks = weeks_to_distance(f.longest_m, goal.distance_m);
    parts.push(Part {
        label: tr!("Distância", "Distance").into(),
        now: format::km(f.longest_m),
        goal: format::km(goal.distance_m),
        fraction: (f.longest_m / goal.distance_m).min(1.0) as f32,
        weeks: dist_weeks,
        done: f.longest_m >= goal.distance_m * 0.98,
    });
    if let Some(t) = goal.target_time_s {
        let pred = f.predicted_time.unwrap_or(f.easy_pace * 0.9 * km);
        let weeks = if pred <= t {
            0.0
        } else {
            (pred / t).ln() / (1.0 / (1.0 - TIME_GAIN_PER_WEEK)).ln()
        };
        parts.push(Part {
            label: tr!("Tempo previsto", "Predicted time").into(),
            now: format::duration(pred),
            goal: format::duration(t),
            fraction: (t / pred).min(1.0) as f32,
            weeks: weeks.max(dist_weeks),
            done: pred <= t,
        });
    }
    if let Some(h) = goal.target_hr {
        let now = f.hr_at_goal_pace;
        let rate = f
            .hr_trend_per_week
            .map(|t| (-t).max(HR_GAIN_PER_WEEK))
            .unwrap_or(HR_GAIN_PER_WEEK);
        let weeks = match now {
            Some(n) if n > h => (n - h) / rate,
            Some(_) => 0.0,
            None => 8.0,
        };
        parts.push(Part {
            label: match goal.goal_pace() {
                Some(p) => trf!("FC no pace {}", "HR at {} pace", format::pace_unit(p)),
                None => tr!("FC no ritmo leve", "HR at easy pace").into(),
            },
            now: format::bpm(now),
            goal: format!("≤ {}", format::bpm(Some(h))),
            fraction: now.map(|n| (h / n).min(1.0) as f32).unwrap_or(0.0),
            weeks,
            done: now.is_some_and(|n| n <= h),
        });
    }
    let weeks = parts
        .iter()
        .map(|p| p.weeks)
        .fold(0.0, f64::max)
        .ceil()
        .clamp(2.0, 104.0) as u32;
    let weeks_available = goal
        .target_date
        .map(|d| ((d - today).num_days().max(0) as f64 / 7.0).round() as u32);
    Projection {
        weeks,
        date: today + Duration::days(weeks as i64 * 7),
        parts,
        weeks_available,
    }
}

/// Running days of the week (0 = Monday), with the long run day included.
pub fn running_days(goal: &Goal) -> Vec<u8> {
    let n = goal.days() as usize;
    let chosen: Vec<u8> = (0..7u8).filter(|d| goal.preferred_days[*d as usize]).collect();
    if chosen.len() == n {
        return chosen;
    }
    // patterns for a Sunday long run, then rotated to the chosen long day
    let base: &[u8] = match n {
        2 => &[2, 6],
        3 => &[1, 3, 6],
        4 => &[1, 2, 4, 6],
        5 => &[0, 1, 3, 4, 6],
        _ => &[0, 1, 2, 3, 4, 6],
    };
    let shift = (goal.long_run_day.min(6) as i32 - 6).rem_euclid(7);
    let mut days: Vec<u8> = base.iter().map(|d| ((*d as i32 + shift).rem_euclid(7)) as u8).collect();
    days.sort();
    days
}

fn easy_pace_range(f: &Fitness) -> (f64, f64) {
    (f.easy_pace, f.easy_pace + 45.0)
}

fn step(repeat: u32, pt: &str, en: &str, secs: f64, pace: Option<(f64, f64)>, hr: Option<f64>) -> Step {
    Step {
        repeat,
        label_pt: pt.into(),
        label_en: en.into(),
        duration_s: secs,
        pace,
        hr_max: hr,
    }
}

fn dist_for(secs: f64, pace: (f64, f64)) -> f64 {
    secs / ((pace.0 + pace.1) / 2.0) * 1000.0
}

#[allow(clippy::too_many_arguments)]
fn make(
    kind: SessionKind,
    date: NaiveDate,
    week: u32,
    phase: Phase,
    minutes: f64,
    goal: &Goal,
    f: &Fitness,
    zones_top3: f64,
    quality_reps: u32,
) -> Session {
    let easy = easy_pace_range(f);
    let gp = goal.goal_pace().unwrap_or(f.easy_pace - 30.0);
    let treadmill = match goal.surface {
        Surface::Treadmill => true,
        Surface::Road => false,
        Surface::Any => matches!(kind, SessionKind::Intervals | SessionKind::Check),
    };
    let mut s = Session {
        date,
        kind,
        phase,
        week,
        duration_s: minutes * 60.0,
        distance_m: 0.0,
        pace: None,
        hr_max: None,
        steps: Vec::new(),
        treadmill,
    };
    match kind {
        SessionKind::Easy => {
            s.pace = Some(easy);
            s.hr_max = Some(f.easy_hr_cap);
            s.distance_m = dist_for(s.duration_s, easy);
        }
        SessionKind::Long => {
            let p = (easy.0 + 10.0, easy.1 + 15.0);
            s.pace = Some(p);
            s.hr_max = Some(f.easy_hr_cap);
            s.distance_m = dist_for(s.duration_s, p);
        }
        SessionKind::Tempo => {
            let tempo_pace = f
                .model
                .and_then(|m| m.pace_at_hr(zones_top3 - 4.0))
                .unwrap_or(f.easy_pace - 35.0)
                .max(gp - 10.0);
            let main = (minutes - 20.0).clamp(10.0, 25.0) * 60.0;
            let tp = (tempo_pace - 5.0, tempo_pace + 10.0);
            s.steps = vec![
                step(
                    1,
                    "Aquecimento leve",
                    "Easy warm-up",
                    600.0,
                    Some(easy),
                    Some(f.easy_hr_cap),
                ),
                step(
                    1,
                    "Ritmo moderado (zona 3)",
                    "Steady (zone 3)",
                    main,
                    Some(tp),
                    Some(zones_top3),
                ),
                step(
                    1,
                    "Desaquecimento leve",
                    "Easy cool-down",
                    600.0,
                    Some(easy),
                    Some(f.easy_hr_cap),
                ),
            ];
            s.duration_s = 1200.0 + main;
            s.distance_m = dist_for(1200.0, easy) + dist_for(main, tp);
            s.hr_max = Some(zones_top3);
            s.pace = Some(tp);
        }
        SessionKind::Intervals => {
            let gpr = (gp - 5.0, gp + 5.0);
            s.steps = vec![
                step(
                    1,
                    "Aquecimento leve",
                    "Easy warm-up",
                    600.0,
                    Some(easy),
                    Some(f.easy_hr_cap),
                ),
                step(
                    quality_reps,
                    "Forte, no pace da meta",
                    "Hard, at goal pace",
                    180.0,
                    Some(gpr),
                    None,
                ),
                step(
                    quality_reps,
                    "Trote ou caminhada",
                    "Jog or walk",
                    120.0,
                    Some((easy.1, easy.1 + 90.0)),
                    None,
                ),
                step(
                    1,
                    "Desaquecimento leve",
                    "Easy cool-down",
                    600.0,
                    Some(easy),
                    Some(f.easy_hr_cap),
                ),
            ];
            let reps = quality_reps as f64;
            s.duration_s = 1200.0 + reps * 300.0;
            s.distance_m =
                dist_for(1200.0, easy) + reps * (dist_for(180.0, gpr) + dist_for(120.0, (easy.1, easy.1 + 90.0)));
            s.pace = Some(gpr);
        }
        SessionKind::Check | SessionKind::GoalAttempt => {
            // check runs cover part of the goal distance at the goal pace
            let target = if kind == SessionKind::GoalAttempt {
                goal.distance_m
            } else {
                (minutes * 60.0 / gp * 1000.0).min(goal.distance_m)
            };
            let target = (target / 500.0).round().max(2.0) * 500.0;
            let p = match goal.goal_pace() {
                Some(g) => (g - 5.0, g + 5.0),
                None => easy,
            };
            s.steps = vec![
                step(
                    1,
                    "Aquecimento leve",
                    "Easy warm-up",
                    480.0,
                    Some(easy),
                    Some(f.easy_hr_cap),
                ),
                step(
                    1,
                    &format!("{} no pace alvo", format::km(target)),
                    &format!("{} at target pace", format::km(target)),
                    target / 1000.0 * (p.0 + p.1) / 2.0,
                    Some(p),
                    goal.target_hr,
                ),
            ];
            s.duration_s = 480.0 + target / 1000.0 * (p.0 + p.1) / 2.0;
            s.distance_m = target + dist_for(480.0, easy);
            s.pace = Some(p);
            s.hr_max = goal.target_hr;
        }
    }
    s
}

/// Builds the sessions from `today` until the goal date (or the projected one).
pub fn generate(
    goal: &Goal,
    f: &Fitness,
    adj: &Adjust,
    zones_top3: f64,
    today: NaiveDate,
    reached: bool,
) -> (Vec<Session>, Projection) {
    let proj = project(goal, f, today);
    let horizon_weeks = if reached {
        4
    } else {
        match goal.target_date {
            Some(d) if d > today => (((d - week_start(today)).num_days() / 7) + 1) as u32,
            _ => proj.weeks + 1,
        }
    }
    .clamp(2, MAX_WEEKS);
    let end_date = match (reached, goal.target_date) {
        (false, Some(d)) if d > today => d,
        _ => week_start(today) + Duration::days(horizon_weeks as i64 * 7 - 1),
    };
    let days = running_days(goal);
    let n = days.len();
    let easy_min_pace = f.easy_pace / 60.0; // minutes per km
    let long_target_min = (goal.distance_m / 1000.0 * 1.05 * easy_min_pace).max(30.0);
    let peak = long_target_min + (n as f64 - 1.0) * 35.0;
    // start from what was actually run: the last full week or the 3-week
    // average, whichever is higher (a lighter week does not reset progress)
    let mut weekly = f.weekly_minutes.max(f.last_week_minutes).max(n as f64 * 20.0).min(peak);
    let mut long = (f.longest_s / 60.0).max(20.0).min(long_target_min);
    let week_offset = ((week_start(today) - week_start(goal.start_date)).num_days() / 7).max(0) as u32;
    // a few weeks of easy running before any quality session
    let base_weeks: u32 = if f.runs_28d < 6 || f.longest_m < goal.distance_m * 0.5 {
        3
    } else {
        1
    };
    let mut out = Vec::new();
    let mut growth_weeks = 0u32;
    let total_weeks = ((end_date - week_start(today)).num_days() / 7 + 1) as u32;
    for wk in 0..total_weeks {
        let ws = week_start(today) + Duration::days(wk as i64 * 7);
        let week_no = week_offset + wk + 1;
        let last_week = ws + Duration::days(6) >= end_date;
        let phase = if reached {
            Phase::Maintain
        } else if last_week && total_weeks >= 3 {
            Phase::Taper
        } else if week_no <= base_weeks {
            Phase::Base
        } else {
            Phase::Build
        };
        let deload = week_no.is_multiple_of(4) || (wk == 0 && adj.deload_now);
        // the current week is already the next step after the runs done so far
        if !deload {
            weekly = (weekly * adj.growth).min(peak);
            if !(adj.hold_long && wk == 0) {
                long = (long * 1.10).min(long_target_min);
            }
            growth_weeks += 1;
        }
        let factor = match (deload, phase) {
            (_, Phase::Taper) => 0.7,
            (true, _) => 0.8,
            _ => 1.0,
        };
        let w_min = weekly * factor;
        let l_min = (long * if deload { 0.85 } else { 1.0 }).min(w_min * 0.5).max(20.0);
        let check_week = goal.goal_pace().is_some() && week_no.is_multiple_of(3) && phase != Phase::Taper;
        let quality = matches!(phase, Phase::Build | Phase::Maintain) && n >= 3 && !deload;
        let quality_kind = if goal.target_time_s.is_some() {
            SessionKind::Intervals
        } else {
            SessionKind::Tempo
        };
        let reps = (4 + growth_weeks / 3).min(6);
        // roles: long on the long-run day; quality/check on the day farthest from it
        let long_day = if days.contains(&goal.long_run_day) {
            goal.long_run_day
        } else {
            *days.last().unwrap()
        };
        let special_day = days.iter().copied().filter(|d| *d != long_day).max_by_key(|d| {
            let gap = (*d as i32 - long_day as i32).rem_euclid(7);
            gap.min(7 - gap)
        });
        // the special session (check or quality) only fits when the week has
        // room for it plus at least 20 minutes for each easy run
        let wanted = if check_week {
            Some(SessionKind::Check)
        } else if quality {
            Some(quality_kind)
        } else {
            None
        }
        .filter(|_| special_day.is_some());
        let n_easy_with = n.saturating_sub(2);
        let budget = w_min - l_min - 20.0 * n_easy_with as f64;
        let max_reps = ((budget - 20.0) / 5.0).floor().max(0.0) as u32;
        let special = match wanted {
            Some(SessionKind::Intervals) if max_reps >= 3 => {
                Some((SessionKind::Intervals, budget.min(50.0), reps.min(max_reps)))
            }
            Some(SessionKind::Tempo) if budget >= 32.0 => Some((SessionKind::Tempo, budget.min(45.0), reps)),
            // the check is what measures progress: it may replace an easy run whole
            Some(SessionKind::Check) if budget >= 12.0 => {
                Some((SessionKind::Check, (budget - 8.0).clamp(10.0, 40.0), reps))
            }
            _ => None,
        };
        let mut week_sessions: Vec<Session> = Vec::new();
        for &d in &days {
            let date = ws + Duration::days(d as i64);
            if date < today || date > end_date {
                continue;
            }
            let s = if phase == Phase::Taper && d == long_day {
                make(
                    SessionKind::GoalAttempt,
                    date,
                    week_no,
                    phase,
                    0.0,
                    goal,
                    f,
                    zones_top3,
                    reps,
                )
            } else if d == long_day {
                make(
                    SessionKind::Long,
                    date,
                    week_no,
                    phase,
                    l_min.round(),
                    goal,
                    f,
                    zones_top3,
                    reps,
                )
            } else if let (true, Some((k, m, r))) = (Some(d) == special_day, special) {
                make(k, date, week_no, phase, m.round(), goal, f, zones_top3, r)
            } else {
                make(SessionKind::Easy, date, week_no, phase, 0.0, goal, f, zones_top3, reps)
            };
            week_sessions.push(s);
        }
        // easy runs share what is left of the week's time (planned for the whole
        // week, so a week that starts today is not crammed)
        let n_easy_total = days.len() - 1 - usize::from(special.is_some());
        let fixed: f64 = days
            .iter()
            .filter(|d| **d == long_day || (special.is_some() && Some(**d) == special_day))
            .map(|d| {
                week_sessions
                    .iter()
                    .find(|s| s.date == ws + Duration::days(*d as i64))
                    .map(|s| s.duration_s / 60.0)
                    .unwrap_or(if *d == long_day {
                        l_min
                    } else {
                        special.map(|x| x.1).unwrap_or(0.0)
                    })
            })
            .sum();
        let easy_min = ((w_min - fixed) / n_easy_total.max(1) as f64).clamp(15.0, 60.0).round();
        for s in week_sessions.iter_mut().filter(|s| s.kind == SessionKind::Easy) {
            *s = make(
                SessionKind::Easy,
                s.date,
                week_no,
                phase,
                easy_min,
                goal,
                f,
                zones_top3,
                reps,
            );
        }
        out.extend(week_sessions);
    }
    (out, proj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coach::fitness::HrModel;
    use chrono::Datelike;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn beginner() -> Fitness {
        Fitness {
            runs_28d: 4,
            weekly_minutes: 60.0,
            weekly_km: 8.0,
            last_week_minutes: 60.0,
            longest_m: 3000.0,
            longest_s: 1500.0,
            model: Some(HrModel {
                a: 70.0,
                b: 30.0,
                points: 20,
            }),
            easy_pace: 450.0,
            easy_hr_cap: 145.0,
            predicted_time: Some(2100.0),
            hr_at_goal_pace: Some(175.0),
            hr_trend_per_week: Some(-0.8),
            hr_history: vec![],
        }
    }

    fn weekly_minutes(s: &[Session]) -> Vec<f64> {
        let mut m: std::collections::BTreeMap<u32, f64> = Default::default();
        for x in s {
            *m.entry(x.week).or_default() += x.duration_s / 60.0;
        }
        m.into_values().collect()
    }

    #[test]
    fn days_patterns() {
        let g = Goal {
            days_per_week: 3,
            long_run_day: 6,
            ..Default::default()
        };
        assert_eq!(running_days(&g), vec![1, 3, 6]);
        let g = Goal {
            days_per_week: 3,
            long_run_day: 5,
            ..Default::default()
        };
        assert_eq!(running_days(&g), vec![0, 2, 5]);
        let mut chosen = [false; 7];
        chosen[0] = true;
        chosen[3] = true;
        let g = Goal {
            days_per_week: 2,
            preferred_days: chosen,
            ..Default::default()
        };
        assert_eq!(running_days(&g), vec![0, 3]);
    }

    #[test]
    fn plan_respects_days_growth_and_ends_with_the_goal() {
        let today = d("2026-10-19"); // Monday
        let goal = Goal {
            distance_m: 5000.0,
            target_time_s: Some(1800.0),
            days_per_week: 3,
            start_date: today,
            ..Default::default()
        };
        let (s, proj) = generate(&goal, &beginner(), &Adjust::default(), 165.0, today, false);
        assert!(proj.weeks >= 2);
        // three runs a week on Tue, Thu, Sun
        for x in &s {
            assert!([1, 3, 6].contains(&x.date.weekday().num_days_from_monday()), "{x:?}");
        }
        // weekly time never grows more than 10% over the busiest week so far
        // (coming back from a lighter week is not growth)
        let w = weekly_minutes(&s);
        for k in 1..w.len() {
            let before = w[..k].iter().cloned().fold(0.0, f64::max);
            assert!(w[k] <= before * 1.10 + 3.0, "week {k}: {w:?}");
        }
        // no quality session in the base weeks, intervals later, goal attempt last
        assert!(
            s.iter()
                .filter(|x| x.week <= 3)
                .all(|x| !matches!(x.kind, SessionKind::Intervals | SessionKind::Tempo))
        );
        assert!(s.iter().any(|x| x.kind == SessionKind::Intervals));
        assert!(s.iter().any(|x| x.kind == SessionKind::Check));
        assert_eq!(s.last().unwrap().kind, SessionKind::GoalAttempt);
        assert!((s.last().unwrap().distance_m - 5000.0).abs() < 2000.0);
        // easy sessions carry an HR cap and a treadmill-friendly pace range
        let e = s.iter().find(|x| x.kind == SessionKind::Easy).unwrap();
        assert_eq!(e.hr_max, Some(145.0));
        assert!(e.pace_text().unwrap().contains("km/h"));
        // the long run never passes the goal distance by much
        assert!(
            s.iter()
                .filter(|x| x.kind == SessionKind::Long)
                .all(|x| x.distance_m <= 5000.0 * 1.15)
        );
    }

    #[test]
    fn target_date_bounds_the_plan_and_adjustments_matter() {
        let today = d("2026-10-19");
        let goal = Goal {
            target_date: Some(d("2026-11-29")),
            start_date: today,
            ..Default::default()
        };
        let (s, _) = generate(&goal, &beginner(), &Adjust::default(), 165.0, today, false);
        assert!(s.iter().all(|x| x.date <= d("2026-11-29")));
        let held = Adjust {
            growth: 1.0,
            ..Default::default()
        };
        let (s2, _) = generate(&goal, &beginner(), &held, 165.0, today, false);
        let a: f64 = s.iter().map(|x| x.duration_s).sum();
        let b: f64 = s2.iter().map(|x| x.duration_s).sum();
        assert!(b < a, "{b} < {a}");
        // reached: maintenance only
        let (m, _) = generate(&goal, &beginner(), &Adjust::default(), 165.0, today, true);
        assert!(m.iter().all(|x| x.phase == Phase::Maintain));
    }

    #[test]
    fn projection_parts() {
        let today = d("2026-10-19");
        let goal = Goal {
            distance_m: 5000.0,
            target_hr: Some(165.0),
            hr_pace: Some(360.0),
            ..Default::default()
        };
        let p = project(&goal, &beginner(), today);
        // distance 3 → 5 km and 10 bpm to drop at 0.8 bpm/week
        assert_eq!(p.parts.len(), 2);
        assert!(p.parts[1].weeks > 12.0 && p.parts[1].weeks < 13.0, "{:?}", p.parts[1]);
        assert_eq!(p.weeks, 13);
    }

    #[test]
    fn reached_detection() {
        let start = d("2026-10-01");
        let goal = Goal {
            distance_m: 5000.0,
            target_hr: Some(165.0),
            hr_pace: Some(360.0),
            start_date: start,
            ..Default::default()
        };
        let mk = |id, date: &str, hr| Activity {
            id,
            start_time: d(date).and_hms_opt(7, 0, 0).unwrap().and_utc().timestamp(),
            distance_m: Some(5010.0),
            moving_s: Some(5.01 * 358.0),
            avg_hr: Some(hr),
            modality: crate::model::Modality::Treadmill,
            ..Default::default()
        };
        let runs = vec![
            mk(1, "2026-09-20", 160.0),
            mk(2, "2026-10-05", 168.0),
            mk(3, "2026-10-12", 163.0),
        ];
        assert_eq!(goal_reached(&goal, &runs, &HashMap::new()), Some((d("2026-10-12"), 3)));
    }
}
