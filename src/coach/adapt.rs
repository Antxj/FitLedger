//! How the last weeks went, and what the coach changes because of it.
//! Every adjustment carries the data that triggered it.

use super::fitness::day;
use super::{Session, SessionKind, Status};
use crate::analysis::metrics::Metrics;
use crate::format;
use crate::model::Activity;
use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Normal week-to-week growth of the running time (≤ 10%).
pub const GROWTH: f64 = 1.08;
/// Growth when easy runs are too hard.
pub const GROWTH_CAREFUL: f64 = 1.04;

/// A planned session as stored, with what happened.
#[derive(Debug, Clone, PartialEq)]
pub struct Tracked {
    pub id: i64,
    pub session: Session,
    pub status: Status,
    pub activity_id: Option<i64>,
}

/// Links runs to planned sessions (same day first, then ±1 day) and sets the
/// status of every session up to today. Returns the ids that changed.
pub fn match_runs(sessions: &mut [Tracked], runs: &[Activity], today: NaiveDate) -> Vec<i64> {
    let mut changed = Vec::new();
    let mut used: HashSet<i64> = sessions.iter().filter_map(|s| s.activity_id).collect();
    // forget links to runs that no longer exist
    let ids: HashSet<i64> = runs.iter().map(|r| r.id).collect();
    for s in sessions.iter_mut() {
        if let Some(a) = s.activity_id
            && !ids.contains(&a)
        {
            s.activity_id = None;
            s.status = Status::Planned;
            used.remove(&a);
            changed.push(s.id);
        }
    }
    for pass in 0..2 {
        for s in sessions
            .iter_mut()
            .filter(|s| s.activity_id.is_none() && s.session.date <= today)
        {
            let found = runs.iter().filter(|r| !used.contains(&r.id)).find(|r| {
                let diff = (day(r) - s.session.date).num_days().abs();
                if pass == 0 { diff == 0 } else { diff == 1 }
            });
            if let Some(r) = found {
                used.insert(r.id);
                s.activity_id = Some(r.id);
                changed.push(s.id);
            }
        }
    }
    for s in sessions.iter_mut() {
        let new = match s.activity_id.and_then(|id| runs.iter().find(|r| r.id == id)) {
            Some(r) => {
                let planned = s.session.duration_s.max(1.0);
                let done = r.duration().unwrap_or(0.0);
                if done >= planned * 0.7 || r.distance().unwrap_or(0.0) >= s.session.distance_m * 0.7 {
                    Status::Done
                } else {
                    Status::Partial
                }
            }
            None if s.session.date < today => Status::Missed,
            None => Status::Planned,
        };
        if new != s.status {
            s.status = new;
            if !changed.contains(&s.id) {
                changed.push(s.id);
            }
        }
    }
    changed
}

/// What the coach noticed. Rendered in the current language when shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Note {
    OnTrack { done: usize, planned: usize },
    Missed { missed: usize, planned: usize },
    EasyTooHard { hard: usize, easy: usize },
    HighDrift { pct: f64, date: NaiveDate },
    Fatigue { date: NaiveDate },
    NoData,
    GoalReached { date: NaiveDate },
    GoalChanged,
    Ambitious { weeks_needed: u32, weeks_available: u32 },
}

impl Note {
    pub fn text(&self) -> String {
        match self {
            Note::OnTrack { done, planned } => trf!(
                "Você fez {done} de {planned} treinos nas últimas 2 semanas. O plano segue a progressão normal.",
                "You did {done} of {planned} sessions in the last 2 weeks. The plan keeps its normal progression."
            ),
            Note::Missed { missed, planned } => trf!(
                "{missed} de {planned} treinos não foram feitos nas últimas 2 semanas. O volume não aumenta esta semana; nada é \"compensado\".",
                "{missed} of {planned} sessions were missed in the last 2 weeks. Volume does not increase this week; nothing is \"made up\"."
            ),
            Note::EasyTooHard { hard, easy } => trf!(
                "{hard} de {easy} corridas leves passaram do limite de FC. Corra mais devagar nelas (ou caminhe um pouco): o crescimento do volume ficou mais lento.",
                "{hard} of {easy} easy runs went over the HR limit. Run them slower (or walk a little): volume now grows more slowly."
            ),
            Note::HighDrift { pct, date } => trf!(
                "Deriva cardíaca de {}% no longão de {}. O longão não aumenta até ela cair.",
                "Cardiac drift of {}% on the long run of {}. The long run does not grow until it drops.",
                format::num(*pct, 1),
                format::date(*date)
            ),
            Note::Fatigue { date } => trf!(
                "Você registrou cansaço alto em {}. Esta semana fica mais leve.",
                "You logged high fatigue on {}. This week is lighter.",
                format::date(*date)
            ),
            Note::NoData => tr!(
                "Ainda não há corridas suficientes: o plano começa leve e se ajusta assim que você importar os primeiros treinos.",
                "Not enough runs yet: the plan starts easy and adjusts as soon as you import your first runs."
            )
            .into(),
            Note::GoalReached { date } => trf!(
                "Meta atingida em {}! O plano passou para manutenção. Que tal uma meta nova?",
                "Goal reached on {}! The plan switched to maintenance. How about a new goal?",
                format::date(*date)
            ),
            Note::GoalChanged => tr!("Meta alterada: o plano foi refeito a partir de hoje.", "Goal changed: the plan was rebuilt from today.").into(),
            Note::Ambitious { weeks_needed, weeks_available } => trf!(
                "A data da meta é apertada: a estimativa é de {weeks_needed} semanas e há {weeks_available}. O plano não acelera além do seguro; ele mira a data, mas pode ser preciso adiá-la.",
                "The goal date is tight: the estimate is {weeks_needed} weeks and there are {weeks_available}. The plan does not speed up beyond what is safe; it aims at the date, but it may need to move."
            ),
        }
    }

    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            Note::Missed { .. }
                | Note::EasyTooHard { .. }
                | Note::HighDrift { .. }
                | Note::Fatigue { .. }
                | Note::Ambitious { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Adjust {
    pub growth: f64,
    pub hold_long: bool,
    pub deload_now: bool,
    pub notes: Vec<Note>,
    /// Data behind the notes, one line each.
    pub evidence: Vec<String>,
}

impl Default for Adjust {
    fn default() -> Self {
        Adjust {
            growth: GROWTH,
            hold_long: false,
            deload_now: false,
            notes: Vec::new(),
            evidence: Vec::new(),
        }
    }
}

fn line(a: &Activity) -> String {
    format!(
        "{} · {} · {} · {} · {}",
        format::date(day(a)),
        a.modality.label(),
        format::opt_km(a.distance()),
        format::opt_pace(a.pace_s_per_km()),
        format::bpm(a.avg_hr)
    )
}

/// Share of the run spent above zone 2 (from the stored zone times).
fn share_above_easy(m: &Metrics) -> Option<f64> {
    let total: f64 = m.zones_s.iter().sum();
    (total > 60.0).then(|| (m.zones_s[3] + m.zones_s[4] + m.zones_s[5]) / total)
}

pub fn review(sessions: &[Tracked], runs: &[Activity], metrics: &HashMap<i64, Metrics>, today: NaiveDate) -> Adjust {
    let mut adj = Adjust::default();
    let from = today - Duration::days(14);
    let past: Vec<&Tracked> = sessions
        .iter()
        .filter(|s| s.session.date >= from && s.session.date < today)
        .collect();
    let recent_runs: Vec<&Activity> = runs.iter().filter(|r| day(r) >= from && day(r) <= today).collect();
    if runs.is_empty() {
        adj.notes.push(Note::NoData);
        return adj;
    }

    let missed = past.iter().filter(|s| s.status == Status::Missed).count();
    let done = past
        .iter()
        .filter(|s| matches!(s.status, Status::Done | Status::Partial))
        .count();
    if past.len() >= 2 && missed * 2 >= past.len() {
        adj.growth = 1.0;
        adj.notes.push(Note::Missed {
            missed,
            planned: past.len(),
        });
        for s in past.iter().filter(|s| s.status == Status::Missed) {
            adj.evidence.push(format!(
                "{} · {} · {}",
                format::date(s.session.date),
                s.session.kind.label(),
                Status::Missed.label()
            ));
        }
    }

    // easy runs: planned as easy/long, or unplanned runs
    let easy_ids: HashSet<i64> = sessions
        .iter()
        .filter(|s| matches!(s.session.kind, SessionKind::Easy | SessionKind::Long))
        .filter_map(|s| s.activity_id)
        .collect();
    let planned_hard: HashSet<i64> = sessions
        .iter()
        .filter(|s| s.session.kind.is_hard())
        .filter_map(|s| s.activity_id)
        .collect();
    let easy_runs: Vec<&&Activity> = recent_runs
        .iter()
        .filter(|r| easy_ids.contains(&r.id) || !planned_hard.contains(&r.id))
        .collect();
    let too_hard: Vec<&&&Activity> = easy_runs
        .iter()
        .filter(|r| metrics.get(&r.id).and_then(share_above_easy).is_some_and(|s| s > 0.3))
        .collect();
    if easy_runs.len() >= 2 && too_hard.len() * 2 >= easy_runs.len() {
        adj.growth = adj.growth.min(GROWTH_CAREFUL);
        adj.notes.push(Note::EasyTooHard {
            hard: too_hard.len(),
            easy: easy_runs.len(),
        });
        for r in &too_hard {
            let share = metrics.get(&r.id).and_then(share_above_easy).unwrap_or(0.0);
            adj.evidence.push(trf!(
                "{} · {}% acima da zona 2",
                "{} · {}% above zone 2",
                line(r),
                format::num(share * 100.0, 0)
            ));
        }
    }

    // long run drift: the latest run of 40+ minutes
    if let Some(r) = recent_runs
        .iter()
        .filter(|r| r.duration().unwrap_or(0.0) >= 2400.0)
        .max_by_key(|r| r.start_time)
        && let Some(d) = metrics
            .get(&r.id)
            .and_then(|m| m.drift.as_ref())
            .and_then(|d| d.decoupling_pct)
        && d > 8.0
    {
        adj.hold_long = true;
        adj.notes.push(Note::HighDrift { pct: d, date: day(r) });
        adj.evidence
            .push(trf!("{} · deriva {}%", "{} · drift {}%", line(r), format::num(d, 1)));
    }

    // fatigue logged in the last 7 days
    if let Some(r) = runs
        .iter()
        .filter(|r| day(r) > today - Duration::days(7) && day(r) <= today)
        .filter(|r| {
            r.fatigue.is_some_and(|f| f >= 4)
                || r.soreness.is_some_and(|f| f >= 4)
                || (easy_ids.contains(&r.id) && r.rpe.is_some_and(|x| x >= 8))
        })
        .max_by_key(|r| r.start_time)
    {
        adj.deload_now = true;
        adj.notes.push(Note::Fatigue { date: day(r) });
        adj.evidence.push(format!(
            "{} · RPE {} · {} {} · {} {}",
            line(r),
            r.rpe.map(|v| v.to_string()).unwrap_or_else(format::dash),
            tr!("fadiga", "fatigue"),
            r.fatigue.map(|v| v.to_string()).unwrap_or_else(format::dash),
            tr!("dor", "soreness"),
            r.soreness.map(|v| v.to_string()).unwrap_or_else(format::dash)
        ));
    }

    if adj.notes.is_empty() && !past.is_empty() {
        adj.notes.push(Note::OnTrack {
            done,
            planned: past.len(),
        });
    }
    adj
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coach::Phase;
    use crate::model::Modality;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn sess(id: i64, date: &str, kind: SessionKind) -> Tracked {
        Tracked {
            id,
            session: Session {
                date: d(date),
                kind,
                phase: Phase::Base,
                week: 1,
                duration_s: 1800.0,
                distance_m: 4000.0,
                pace: None,
                hr_max: Some(145.0),
                steps: vec![],
                treadmill: false,
            },
            status: Status::Planned,
            activity_id: None,
        }
    }

    fn run(id: i64, date: &str, secs: f64) -> Activity {
        Activity {
            id,
            start_time: d(date).and_hms_opt(7, 0, 0).unwrap().and_utc().timestamp(),
            modality: Modality::RoadRun,
            distance_m: Some(secs / 450.0 * 1000.0),
            moving_s: Some(secs),
            avg_hr: Some(150.0),
            ..Default::default()
        }
    }

    #[test]
    fn matching_and_status() {
        let today = d("2026-10-19");
        let mut s = vec![
            sess(1, "2026-10-13", SessionKind::Easy),
            sess(2, "2026-10-15", SessionKind::Easy),
            sess(3, "2026-10-18", SessionKind::Long),
            sess(4, "2026-10-20", SessionKind::Easy),
        ];
        let runs = vec![run(10, "2026-10-13", 1900.0), run(11, "2026-10-16", 600.0)];
        match_runs(&mut s, &runs, today);
        assert_eq!((s[0].status, s[0].activity_id), (Status::Done, Some(10)));
        assert_eq!((s[1].status, s[1].activity_id), (Status::Partial, Some(11))); // one day late and short
        assert_eq!(s[2].status, Status::Missed);
        assert_eq!(s[3].status, Status::Planned);
        // the run is deleted: link and status go back
        let changed = match_runs(&mut s, &runs[..1], today);
        assert!(changed.contains(&2));
        assert_eq!(s[1].status, Status::Missed);
    }

    #[test]
    fn missed_sessions_stop_growth_and_easy_too_hard_slows_it() {
        let today = d("2026-10-19");
        let mut s = vec![
            sess(1, "2026-10-08", SessionKind::Easy),
            sess(2, "2026-10-10", SessionKind::Easy),
            sess(3, "2026-10-13", SessionKind::Easy),
            sess(4, "2026-10-15", SessionKind::Easy),
        ];
        let runs = vec![run(10, "2026-10-08", 1800.0), run(11, "2026-10-10", 1800.0)];
        match_runs(&mut s, &runs, today);
        let mut metrics = HashMap::new();
        for id in [10, 11] {
            metrics.insert(
                id,
                Metrics {
                    zones_s: [0.0, 0.0, 600.0, 900.0, 300.0, 0.0],
                    ..Default::default()
                },
            );
        }
        let a = review(&s, &runs, &metrics, today);
        assert_eq!(a.growth, 1.0);
        assert!(
            a.notes
                .iter()
                .any(|n| matches!(n, Note::Missed { missed: 2, planned: 4 }))
        );
        assert!(a.notes.iter().any(|n| matches!(n, Note::EasyTooHard { .. })));
        assert!(!a.evidence.is_empty());
        assert!(a.notes.iter().all(|n| !n.text().is_empty()));
    }

    #[test]
    fn fatigue_triggers_deload() {
        let today = d("2026-10-19");
        let mut r = run(10, "2026-10-18", 1800.0);
        r.fatigue = Some(5);
        let a = review(&[], &[r], &HashMap::new(), today);
        assert!(a.deload_now);
    }
}
