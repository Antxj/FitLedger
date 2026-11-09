//! Goal projection and the queue of next sessions.
//!
//! There are no fixed days: the runner trains when they can and every run
//! fulfils the next session of the queue. The queue is a pure function of
//! (goal, fitness, adjustments, what was done, today), so calling it again
//! after a new run or a goal change rebuilds it consistently. Rules are
//! conservative for a new runner: sessions are sized from the runs actually
//! done (easy +5%, long run +10% at a time), every third cycle of four
//! sessions is lighter, a long break or a load spike makes the next session
//! smaller, and harder sessions only start after a few easy ones.

use super::adapt::{Adjust, GROWTH, GROWTH_CAREFUL};
use super::fitness::{Fitness, day};
use super::{Goal, Phase, Session, SessionKind, Step, Surface};
use crate::format;
use crate::model::Activity;
use chrono::{Duration, NaiveDate};
use std::collections::HashMap;

/// Sessions shown ahead.
pub const QUEUE: usize = 3;
/// Days between progress checks.
const CHECK_EVERY_DAYS: i64 = 18;
/// Easy pace (s/km) from which easy runs become run/walk.
pub const RUN_WALK_PACE: f64 = 540.0;
/// Expected improvement of race time per week for a new runner (3 runs a week).
const TIME_GAIN_PER_WEEK: f64 = 0.008;
/// Expected drop of heart rate at a fixed pace per week when the data has no trend yet (3 runs a week).
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

/// Projection at the runner's current frequency.
pub fn project(goal: &Goal, f: &Fitness, today: NaiveDate) -> Projection {
    project_at(goal, f, today, f.runs_per_week)
}

/// Projection if the runner ran `per_week` times a week. Improvement is
/// assumed to grow with the square root of the frequency (diminishing
/// returns) and long runs to be about 30% of the runs.
pub fn project_at(goal: &Goal, f: &Fitness, today: NaiveDate, per_week: f64) -> Projection {
    let per_week = if per_week > 0.0 { per_week } else { 2.0 };
    let current = if f.runs_per_week > 0.0 { f.runs_per_week } else { 2.0 };
    // relative to 3 runs a week (the default rates) and to now (the measured trend)
    let vs_default = (per_week / 3.0).clamp(0.25, 1.0).sqrt();
    let vs_now = (per_week / current).clamp(0.25, 4.0).sqrt();
    let mut parts = Vec::new();
    let km = goal.distance_m / 1000.0;
    // distance: what the longest recent run says
    let dist_weeks = weeks_to_distance(f.longest_m, goal.distance_m) / (per_week * 0.3).clamp(0.25, 1.0);
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
        let gain = TIME_GAIN_PER_WEEK * vs_default;
        let weeks = if pred <= t {
            0.0
        } else {
            (pred / t).ln() / (1.0 / (1.0 - gain)).ln()
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
            .map(|t| (-t * vs_now).max(HR_GAIN_PER_WEEK * vs_default))
            .unwrap_or(HR_GAIN_PER_WEEK * vs_default);
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
    if matches!(kind, SessionKind::Easy | SessionKind::Long) && f.easy_pace >= RUN_WALK_PACE {
        run_walk(&mut s, f);
    }
    s
}

/// Brisk walk, s/km (5.5–5.0 km/h).
const WALK_PACE: (f64, f64) = (655.0, 720.0);

/// Easy pace so slow that running continuously under the HR cap is hard:
/// jog/walk repeats (Galloway's run-walk). The jog is at the speed where the
/// HR × speed model reaches the cap, up to 40 s/km faster (the walk breaks
/// keep HR from settling); the walk breaks shrink as the easy pace gets
/// faster.
fn run_walk(s: &mut Session, f: &Fitness) {
    let jog = if f.easy_pace >= 630.0 {
        2.0
    } else if f.easy_pace >= 585.0 {
        3.0
    } else {
        5.0
    };
    let reps = (s.duration_s / 60.0 / (jog + 1.0)).round().max(3.0) as u32;
    let jog_pace = (f.easy_pace - 40.0, f.easy_pace);
    s.steps = vec![
        step(
            reps,
            "Trote leve",
            "Easy jog",
            jog * 60.0,
            Some(jog_pace),
            Some(f.easy_hr_cap),
        ),
        step(reps, "Caminhada rápida", "Brisk walk", 60.0, Some(WALK_PACE), None),
    ];
    s.duration_s = reps as f64 * (jog + 1.0) * 60.0;
    s.distance_m = reps as f64 * (dist_for(jog * 60.0, jog_pace) + dist_for(60.0, WALK_PACE));
    // average pace of the whole session, walks included
    let avg = s.duration_s / (s.distance_m / 1000.0);
    s.pace = Some((avg - 15.0, avg + 15.0));
}

/// What the queue needs to know about the plan sessions done so far.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Context {
    /// Plan sessions done since the goal started.
    pub done: usize,
    /// Day of the last run and whether it was demanding (long, quality or mostly above zone 2).
    pub last_run: Option<(NaiveDate, bool)>,
    /// Last progress check done.
    pub last_check: Option<NaiveDate>,
    /// Plan sessions done since the last goal attempt (None = never attempted).
    pub since_attempt: Option<usize>,
    /// The last plan session done was the easy run before the goal attempt.
    pub tapered: bool,
}

/// Easy sessions before any quality session (newer runners get more).
fn base_runs(goal: &Goal, f: &Fitness) -> usize {
    if f.runs_28d < 6 || f.longest_m < goal.distance_m * 0.5 {
        4
    } else {
        2
    }
}

/// The next `QUEUE` sessions. The first one is dated with the earliest day
/// it is advised (a rest day after a demanding run); the others are only
/// ordered.
pub fn generate(
    goal: &Goal,
    f: &Fitness,
    adj: &Adjust,
    zones_top3: f64,
    today: NaiveDate,
    reached: bool,
    ctx: &Context,
) -> (Vec<Session>, Projection) {
    let proj = project(goal, f, today);
    // sizes come from what the runner actually does
    let long_target = (goal.distance_m / 1000.0 * 1.05 * f.easy_pace / 60.0).max(30.0);
    let typical = if f.typical_s > 0.0 { f.typical_s / 60.0 } else { 20.0 };
    let longest = (f.longest_s / 60.0).max(typical);
    let comeback = f.last_run.is_some_and(|d| (today - d).num_days() > 14);
    let (easy_f, long_f): (f64, f64) = if comeback {
        (0.8, 0.85)
    } else if adj.growth >= GROWTH {
        (1.05, 1.10)
    } else if adj.growth >= GROWTH_CAREFUL {
        (1.0, 1.05)
    } else {
        (0.95, 1.0)
    };
    let long_f = if adj.hold_long { long_f.min(1.0) } else { long_f };
    let easy = (typical * easy_f).clamp(15.0, 60.0).min(long_target);
    let long = (longest * long_f).min(long_target).max((easy + 5.0).min(long_target));
    let quality_kind = if goal.target_time_s.is_some() {
        SessionKind::Intervals
    } else {
        SessionKind::Tempo
    };
    let base = base_runs(goal, f);
    let ready = !reached
        && (proj.parts.iter().all(|p| p.weeks <= 1.0)
            || goal
                .target_date
                .is_some_and(|d| d > today && (d - today).num_days() <= 10))
        && ctx.since_attempt.is_none_or(|n| n >= 4);
    // a rest day after a demanding run (and between runs while still new to it)
    let mut first = today;
    if let Some((d, hard)) = ctx.last_run {
        let gap = if hard || f.runs_28d < 6 { 2 } else { 1 };
        first = first.max(d + Duration::days(gap));
    }
    let mut out = Vec::new();
    let mut check_queued = false;
    for i in 0..QUEUE {
        let k = ctx.done + i;
        let date = first + Duration::days(2 * i as i64);
        let cycle = (k / 4) as u32 + 1;
        let light = !reached && k >= base && cycle.is_multiple_of(3);
        let mut phase = if reached {
            Phase::Maintain
        } else if k < base {
            Phase::Base
        } else if light {
            Phase::Light
        } else {
            Phase::Build
        };
        let mut factor = if light { 0.8 } else { 1.0 };
        if i == 0 && adj.deload_now {
            factor *= 0.8;
        }
        let check_due = goal.goal_pace().is_some()
            && !check_queued
            && k >= base
            && ctx.last_check.is_none_or(|d| (date - d).num_days() >= CHECK_EVERY_DAYS);
        let kind = if ready {
            // an easy run, then the goal itself
            phase = Phase::Taper;
            match (ctx.tapered, i) {
                (true, 0) | (false, 1) => SessionKind::GoalAttempt,
                (false, 0) => {
                    factor *= 0.7;
                    SessionKind::Easy
                }
                _ => {
                    phase = Phase::Maintain;
                    SessionKind::Easy
                }
            }
        } else if k < base {
            if k % 2 == 1 {
                SessionKind::Long
            } else {
                SessionKind::Easy
            }
        } else {
            match k % 4 {
                1 => SessionKind::Long,
                3 if check_due => SessionKind::Check,
                // one harder session every four, only with enough running
                3 if f.runs_per_week >= 1.5 && !light => quality_kind,
                _ => SessionKind::Easy,
            }
        };
        check_queued |= kind == SessionKind::Check;
        let minutes = match kind {
            SessionKind::Easy => easy * factor,
            SessionKind::Long => long * factor,
            SessionKind::Tempo => (typical * 1.05).clamp(32.0, 45.0),
            SessionKind::Intervals => (typical * 1.3).max(35.0),
            SessionKind::Check => (typical - 8.0).clamp(10.0, 30.0),
            SessionKind::GoalAttempt => 0.0,
        };
        // intervals: as many 5-minute repeats as fit (3 to 6)
        let reps = (((minutes - 20.0) / 5.0).floor().max(3.0) as u32).min((4 + ctx.done as u32 / 12).min(6));
        out.push(make(
            kind,
            date,
            cycle,
            phase,
            minutes.max(10.0).round(),
            goal,
            f,
            zones_top3,
            reps,
        ));
    }
    (out, proj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coach::fitness::HrModel;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn beginner() -> Fitness {
        Fitness {
            runs_28d: 4,
            weekly_minutes: 60.0,
            weekly_km: 8.0,
            minutes_7d: 60.0,
            typical_s: 1200.0,
            runs_per_week: 3.0,
            last_run: None,
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

    fn ctx(done: usize) -> Context {
        Context {
            done,
            ..Default::default()
        }
    }

    /// Runs ~36 min about once a week, with a high HR.
    fn weekly_runner(today: NaiveDate) -> Fitness {
        Fitness {
            runs_28d: 2,
            runs_per_week: 1.0,
            typical_s: 36.0 * 60.0,
            longest_m: 5000.0,
            longest_s: 36.0 * 60.0,
            easy_pace: 587.0,
            easy_hr_cap: 154.0,
            last_run: Some(today),
            ..beginner()
        }
    }

    #[test]
    fn queue_is_sized_from_the_runs_actually_done() {
        let today = d("2026-11-09");
        let goal = Goal {
            distance_m: 5000.0,
            target_time_s: Some(1800.0),
            target_hr: Some(165.0),
            start_date: d("2026-10-19"),
            ..Default::default()
        };
        let f = weekly_runner(today);
        let c = Context {
            done: 1,
            last_run: Some((today, true)),
            ..Default::default()
        };
        let (q, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, false, &c);
        assert_eq!(q.len(), QUEUE);
        // a long run today: rest tomorrow, next session the day after
        assert_eq!(q[0].date, d("2026-11-11"));
        // the 36-minute habit is respected (no 15-minute runs)
        let e = q.iter().find(|s| s.kind == SessionKind::Easy).unwrap();
        assert!(
            e.duration_s >= 34.0 * 60.0 && e.duration_s <= 42.0 * 60.0,
            "{}",
            e.duration_s
        );
        // easy pace 9:47/km: run/walk, 3 min jog + 1 min walk, HR cap on the jog
        assert!(e.run_walk());
        assert_eq!(e.steps[0].duration_s, 180.0);
        assert_eq!(e.steps[0].hr_max, Some(154.0));
        // a jog speed from the HR model (9:47/km = 6.1 km/h, up to 40 s/km faster) and a walk speed
        assert_eq!(e.steps[0].pace, Some((547.0, 587.0)));
        assert_eq!(e.steps[1].pace, Some(WALK_PACE));
        let avg = e.duration_s / (e.distance_m / 1000.0);
        assert!(avg > 547.0 && avg < 720.0, "{avg}");
        // once a week: no intervals
        assert!(q.iter().all(|s| s.kind != SessionKind::Intervals));
        // the long run is longer than the easy run and capped near the goal distance
        let (q2, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, false, &ctx(5));
        let l = q2.iter().find(|s| s.kind == SessionKind::Long).unwrap();
        assert!(l.duration_s > e.duration_s);
        assert!(l.distance_m <= 5000.0 * 1.15, "{}", l.distance_m);
    }

    #[test]
    fn cycle_quality_checks_and_rest() {
        let today = d("2026-10-19");
        let goal = Goal {
            distance_m: 5000.0,
            target_time_s: Some(1800.0),
            start_date: today,
            ..Default::default()
        };
        let mut f = beginner();
        f.runs_28d = 10;
        f.runs_per_week = 3.0;
        f.typical_s = 1800.0;
        // first sessions: easy and long only
        let (q, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, false, &ctx(0));
        assert!(
            q.iter()
                .all(|s| matches!(s.kind, SessionKind::Easy | SessionKind::Long))
        );
        assert_eq!(q[0].phase, Phase::Base);
        // later, with a check done recently: intervals in the 4th slot
        let c = Context {
            done: 5,
            last_check: Some(today - Duration::days(3)),
            last_run: Some((today - Duration::days(1), false)),
            ..Default::default()
        };
        let (q, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, false, &c);
        assert_eq!(
            q.iter().map(|s| s.kind).collect::<Vec<_>>(),
            vec![SessionKind::Long, SessionKind::Easy, SessionKind::Intervals]
        );
        assert_eq!(q[0].date, today); // easy yesterday: no rest needed
        // the check comes back after 18 days
        let c = Context {
            last_check: Some(today - Duration::days(30)),
            ..c
        };
        let (q, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, false, &c);
        assert_eq!(q[2].kind, SessionKind::Check);
        // every third cycle is lighter
        let (q, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, false, &ctx(8));
        assert!(q.iter().all(|s| s.phase == Phase::Light));
    }

    #[test]
    fn comeback_and_adjustments_shrink_the_next_session() {
        let today = d("2026-11-09");
        let goal = Goal {
            distance_m: 8000.0,
            ..Default::default()
        };
        let mut f = weekly_runner(today);
        f.easy_pace = 450.0; // continuous running
        let (normal, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, false, &ctx(0));
        assert!(!normal[0].run_walk());
        f.last_run = Some(today - Duration::days(20));
        let (back, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, false, &ctx(0));
        assert!(back[0].duration_s < normal[0].duration_s * 0.85);
        f.last_run = Some(today);
        let tired = Adjust {
            deload_now: true,
            ..Default::default()
        };
        let (t, _) = generate(&goal, &f, &tired, 165.0, today, false, &ctx(0));
        assert!(t[0].duration_s < normal[0].duration_s);
    }

    #[test]
    fn ready_means_an_easy_run_then_the_goal() {
        let today = d("2026-11-09");
        let goal = Goal {
            distance_m: 5000.0,
            target_time_s: Some(2400.0),
            ..Default::default()
        };
        let mut f = beginner();
        f.longest_m = 5000.0;
        f.predicted_time = Some(2300.0);
        let (q, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, false, &ctx(10));
        assert_eq!(q[0].kind, SessionKind::Easy);
        assert_eq!(q[0].phase, Phase::Taper);
        assert_eq!(q[1].kind, SessionKind::GoalAttempt);
        let c = Context {
            done: 11,
            tapered: true,
            ..Default::default()
        };
        let (q, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, false, &c);
        assert_eq!(q[0].kind, SessionKind::GoalAttempt);
        // reached: maintenance only
        let (m, _) = generate(&goal, &f, &Adjust::default(), 165.0, today, true, &ctx(12));
        assert!(m.iter().all(|x| x.phase == Phase::Maintain));
    }

    #[test]
    fn more_runs_a_week_bring_the_goal_closer() {
        let today = d("2026-11-09");
        let goal = Goal {
            distance_m: 5000.0,
            target_hr: Some(165.0),
            hr_pace: Some(360.0),
            ..Default::default()
        };
        let mut f = beginner();
        f.runs_per_week = 1.0;
        let now = project(&goal, &f, today);
        let twice = project_at(&goal, &f, today, 2.0);
        assert!(twice.weeks < now.weeks, "{} < {}", twice.weeks, now.weeks);
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
