//! The running coach: from a goal (distance, optional finish time, optional
//! heart-rate cap, days per week) and the runs imported so far, it builds a
//! week-by-week schedule and rebuilds the future part of it every time a run
//! is imported or the goal changes.
//!
//! - `fitness`: what the recent runs say (HR × speed model, volume, longest run...)
//! - `adapt`: how the last weeks went (missed sessions, easy runs too hard...)
//! - `plan`: projection of the goal date and generation of the sessions
//! - `store`: persistence (goal versions, sessions, coach notes)
//!
//! Everything here is conservative and explained to the user. It is training
//! guidance, not medical advice.

pub mod adapt;
pub mod fitness;
pub mod plan;
pub mod store;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// The coach suggests the treadmill for check runs (controlled conditions).
    #[default]
    Any,
    Treadmill,
    Road,
}

impl Surface {
    pub fn label(self) -> &'static str {
        match self {
            Surface::Any => tr!("Rua e esteira", "Road and treadmill"),
            Surface::Treadmill => tr!("Só esteira", "Treadmill only"),
            Surface::Road => tr!("Só rua", "Road only"),
        }
    }
}

/// The runner's goal. Any combination of the optional parts is allowed:
/// "run 10 km", "5 km in 28 min", "5 km at 6:00/km with avg HR ≤ 160"...
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Goal {
    pub id: i64,
    /// Distance to run continuously, meters.
    pub distance_m: f64,
    /// Finish time for the distance, seconds.
    pub target_time_s: Option<f64>,
    /// Average heart rate the run must stay under, bpm.
    pub target_hr: Option<f64>,
    /// Pace for the heart-rate goal (s/km) when there is no finish time.
    pub hr_pace: Option<f64>,
    /// Running days per week (2 to 6).
    pub days_per_week: u8,
    /// Chosen weekdays (Mon..Sun). Ignored unless exactly `days_per_week` are chosen.
    pub preferred_days: [bool; 7],
    /// Weekday of the long run (0 = Monday ... 6 = Sunday).
    pub long_run_day: u8,
    /// Optional deadline; without it the coach estimates one.
    pub target_date: Option<NaiveDate>,
    pub surface: Surface,
    /// First day of this goal version.
    pub start_date: NaiveDate,
}

impl Default for Goal {
    fn default() -> Self {
        Goal {
            id: 0,
            distance_m: 5000.0,
            target_time_s: None,
            target_hr: None,
            hr_pace: None,
            days_per_week: 3,
            preferred_days: [false; 7],
            long_run_day: 6,
            target_date: None,
            surface: Surface::Any,
            start_date: chrono::Local::now().date_naive(),
        }
    }
}

impl Goal {
    /// Pace that matters for the goal: finish-time pace, else the HR-goal pace.
    pub fn goal_pace(&self) -> Option<f64> {
        self.target_time_s
            .map(|t| t / (self.distance_m / 1000.0))
            .or(self.hr_pace)
    }

    pub fn days(&self) -> u8 {
        self.days_per_week.clamp(2, 6)
    }

    /// "5 km · 28:00 · avg HR ≤ 160 bpm"
    pub fn describe(&self) -> String {
        let mut parts = vec![crate::format::km(self.distance_m)];
        if let Some(t) = self.target_time_s {
            parts.push(trf!("em {}", "in {}", crate::format::duration(t)));
        }
        if let Some(h) = self.target_hr {
            match (self.target_time_s, self.hr_pace) {
                (None, Some(p)) => parts.push(trf!(
                    "a {} com FC média ≤ {} bpm",
                    "at {} with avg HR ≤ {} bpm",
                    crate::format::pace_unit(p),
                    h.round()
                )),
                _ => parts.push(trf!("FC média ≤ {} bpm", "avg HR ≤ {} bpm", h.round())),
            }
        }
        parts.join(" · ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    /// Conversational run, heart rate capped at the top of zone 2.
    Easy,
    /// The week's longest run, easy effort.
    Long,
    /// Continuous run in zone 3.
    Tempo,
    /// Repeats at goal pace with easy recoveries.
    Intervals,
    /// Check run: a fixed distance at the goal pace to measure progress.
    Check,
    /// The goal itself.
    GoalAttempt,
}

impl SessionKind {
    pub fn label(self) -> &'static str {
        match self {
            SessionKind::Easy => tr!("Corrida leve", "Easy run"),
            SessionKind::Long => tr!("Longão", "Long run"),
            SessionKind::Tempo => tr!("Ritmo moderado", "Tempo run"),
            SessionKind::Intervals => tr!("Intervalado", "Intervals"),
            SessionKind::Check => tr!("Teste de evolução", "Progress check"),
            SessionKind::GoalAttempt => tr!("Tentativa da meta", "Goal attempt"),
        }
    }

    pub fn is_hard(self) -> bool {
        matches!(
            self,
            SessionKind::Tempo | SessionKind::Intervals | SessionKind::Check | SessionKind::GoalAttempt
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Aerobic base: only easy running.
    Base,
    /// One quality session a week.
    Build,
    /// Easier last week before the goal date.
    Taper,
    /// Goal reached: keep fitness.
    Maintain,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Base => tr!("Base aeróbica", "Aerobic base"),
            Phase::Build => tr!("Desenvolvimento", "Build"),
            Phase::Taper => tr!("Polimento", "Taper"),
            Phase::Maintain => tr!("Manutenção", "Maintenance"),
        }
    }
}

/// A block inside a structured session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub repeat: u32,
    pub label_pt: String,
    pub label_en: String,
    pub duration_s: f64,
    /// Pace range (fast, slow) in s/km.
    pub pace: Option<(f64, f64)>,
    pub hr_max: Option<f64>,
}

impl Step {
    pub fn label(&self) -> &str {
        if crate::i18n::pt() {
            &self.label_pt
        } else {
            &self.label_en
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub date: NaiveDate,
    pub kind: SessionKind,
    pub phase: Phase,
    /// Week number since the start of the goal (1-based).
    pub week: u32,
    pub duration_s: f64,
    /// Expected distance at the target pace (or the distance itself for check runs).
    pub distance_m: f64,
    /// Target pace range (fast, slow), s/km.
    pub pace: Option<(f64, f64)>,
    /// Heart-rate cap, bpm.
    pub hr_max: Option<f64>,
    pub steps: Vec<Step>,
    /// Suggested surface for this session.
    pub treadmill: bool,
}

impl Session {
    /// "~7:10–7:40/km · 8,4–7,8 km/h"
    pub fn pace_text(&self) -> Option<String> {
        let (fast, slow) = self.pace?;
        Some(format!(
            "{}–{}/km · {}–{} km/h",
            crate::format::pace(fast),
            crate::format::pace(slow),
            crate::format::num(crate::format::pace_to_kmh(slow), 1),
            crate::format::num(crate::format::pace_to_kmh(fast), 1),
        ))
    }
}

/// Result of a planned session once its day has passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Planned,
    Done,
    /// Done, but much shorter than planned.
    Partial,
    Missed,
}

impl Status {
    pub fn code(self) -> &'static str {
        match self {
            Status::Planned => "planned",
            Status::Done => "done",
            Status::Partial => "partial",
            Status::Missed => "missed",
        }
    }

    pub fn from_code(s: &str) -> Status {
        match s {
            "done" => Status::Done,
            "partial" => Status::Partial,
            "missed" => Status::Missed,
            _ => Status::Planned,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Planned => tr!("Planejado", "Planned"),
            Status::Done => tr!("Feito", "Done"),
            Status::Partial => tr!("Parcial", "Partial"),
            Status::Missed => tr!("Não feito", "Missed"),
        }
    }
}

/// Monday of the week of `d`.
pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_pace_and_description() {
        let g = Goal {
            distance_m: 5000.0,
            target_time_s: Some(1680.0),
            target_hr: Some(160.0),
            ..Default::default()
        };
        assert_eq!(g.goal_pace(), Some(336.0));
        assert!(g.describe().contains("28:00"));
        let h = Goal {
            target_hr: Some(160.0),
            hr_pace: Some(360.0),
            ..Default::default()
        };
        assert_eq!(h.goal_pace(), Some(360.0));
        assert!(h.describe().contains("6:00/km"));
        assert_eq!(
            Goal {
                days_per_week: 9,
                ..Default::default()
            }
            .days(),
            6
        );
    }
}
