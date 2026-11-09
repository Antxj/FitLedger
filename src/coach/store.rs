//! Persistence of the coach and the `refresh` that ties everything together:
//! link runs to the queue → assess fitness → review → rebuild the queue.

use super::adapt::{self, Adjust, Note, Tracked};
use super::fitness::{self, Fitness, Inputs};
use super::plan::{self, Context, Projection};
use super::{Goal, Phase, Session, SessionKind, Status};
use crate::analysis::metrics;
use crate::db::{Db, Result, now};
use crate::model::Activity;
use chrono::NaiveDate;
use rusqlite::{OptionalExtension, params};

/// (id, created_at, goal, achieved_on) of a goal version.
pub type GoalVersion = (i64, i64, Goal, Option<String>);
/// (time, notes, evidence) of a coach log entry.
pub type LogEntry = (i64, Vec<Note>, Vec<String>);

fn date_str(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Everything the UI shows about the coach.
#[derive(Debug, Clone)]
pub struct CoachState {
    pub goal: Goal,
    pub fitness: Fitness,
    pub projection: Projection,
    pub adjust: Adjust,
    /// Sessions of the goal: the ones done (by date), then the queue.
    pub sessions: Vec<Tracked>,
    pub reached: Option<(NaiveDate, i64)>,
}

impl CoachState {
    /// The next sessions, in order.
    pub fn queue(&self) -> Vec<&Tracked> {
        self.sessions.iter().filter(|s| s.activity_id.is_none()).collect()
    }

    pub fn next_session(&self) -> Option<&Tracked> {
        self.sessions.iter().find(|s| s.activity_id.is_none())
    }

    /// Plan sessions done on `day`.
    pub fn done_on(&self, day: NaiveDate) -> Vec<&Tracked> {
        self.sessions
            .iter()
            .filter(|s| s.activity_id.is_some() && s.session.date == day)
            .collect()
    }
}

impl Db {
    pub fn active_goal(&self) -> Result<Option<Goal>> {
        self.conn
            .query_row(
                "SELECT id, json FROM coach_goal WHERE active = 1 ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()
            .map(|o| {
                o.and_then(|(id, j)| {
                    let mut g: Goal = serde_json::from_str(&j).ok()?;
                    g.id = id;
                    Some(g)
                })
            })
    }

    /// Saves a new version of the goal and makes it the active one.
    /// Past sessions stay attached to the previous version (history).
    pub fn set_goal(&self, goal: &Goal) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE coach_goal SET active = 0 WHERE active = 1", [])?;
        let json = serde_json::to_string(goal).unwrap_or_default();
        tx.execute(
            "INSERT INTO coach_goal(created_at, active, json) VALUES(?1, 1, ?2)",
            params![now(), json],
        )?;
        let id = tx.last_insert_rowid();
        // carry over the sessions already done since the new start date
        if goal.id != 0 {
            tx.execute(
                "UPDATE coach_session SET goal_id = ?1 WHERE goal_id = ?2 AND date >= ?3 AND activity_id IS NOT NULL",
                params![id, goal.id, date_str(goal.start_date)],
            )?;
            tx.execute(
                "DELETE FROM coach_session WHERE goal_id = ?1 AND activity_id IS NULL",
                [goal.id],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }

    /// All goal versions, newest first: (id, created_at, goal, achieved_on).
    pub fn goal_history(&self) -> Result<Vec<GoalVersion>> {
        let mut st = self
            .conn
            .prepare("SELECT id, created_at, json, achieved_on FROM coach_goal ORDER BY id DESC")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        Ok(rows
            .flatten()
            .filter_map(|(id, at, j, ach)| {
                let mut g: Goal = serde_json::from_str(&j).ok()?;
                g.id = id;
                Some((id, at, g, ach))
            })
            .collect())
    }

    pub fn tracked_sessions(&self, goal_id: i64) -> Result<Vec<Tracked>> {
        let mut st = self
            .conn
            .prepare("SELECT id, json, status, activity_id FROM coach_session WHERE goal_id = ?1 ORDER BY date, id")?;
        let rows = st.query_map([goal_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })?;
        Ok(rows
            .flatten()
            .filter_map(|(id, j, st, a)| {
                Some(Tracked {
                    id,
                    session: serde_json::from_str(&j).ok()?,
                    status: Status::from_code(&st),
                    activity_id: a,
                })
            })
            .collect())
    }

    /// Sessions of every goal version (history for streaks and achievements).
    pub fn all_tracked_sessions(&self) -> Result<Vec<Tracked>> {
        let mut st = self
            .conn
            .prepare("SELECT id, json, status, activity_id FROM coach_session ORDER BY date, id")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })?;
        Ok(rows
            .flatten()
            .filter_map(|(id, j, st, a)| {
                Some(Tracked {
                    id,
                    session: serde_json::from_str(&j).ok()?,
                    status: Status::from_code(&st),
                    activity_id: a,
                })
            })
            .collect())
    }

    fn update_tracked(&self, t: &Tracked) -> Result<()> {
        self.conn.execute(
            "UPDATE coach_session SET status = ?2, activity_id = ?3, date = ?4, json = ?5 WHERE id = ?1",
            params![
                t.id,
                t.status.code(),
                t.activity_id,
                date_str(t.session.date),
                serde_json::to_string(&t.session).unwrap_or_default()
            ],
        )?;
        Ok(())
    }

    /// Replaces the queue (every session without a run, including the
    /// "missed" ones of the fixed-day plans before 0.4).
    fn replace_queue(&mut self, goal_id: i64, sessions: &[Session]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM coach_session WHERE goal_id = ?1 AND activity_id IS NULL",
            [goal_id],
        )?;
        for s in sessions {
            tx.execute(
                "INSERT INTO coach_session(goal_id, date, json, status) VALUES(?1, ?2, ?3, 'planned')",
                params![goal_id, date_str(s.date), serde_json::to_string(s).unwrap_or_default()],
            )?;
        }
        tx.commit()
    }

    /// Latest coach notes: (time, notes, evidence).
    pub fn coach_log(&self, goal_id: i64, limit: usize) -> Result<Vec<LogEntry>> {
        let mut st = self
            .conn
            .prepare("SELECT at, notes, evidence FROM coach_log WHERE goal_id = ?1 ORDER BY id DESC LIMIT ?2")?;
        let rows = st.query_map(params![goal_id, limit as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        Ok(rows
            .flatten()
            .map(|(at, n, e)| {
                (
                    at,
                    serde_json::from_str(&n).unwrap_or_default(),
                    serde_json::from_str(&e).unwrap_or_default(),
                )
            })
            .collect())
    }

    fn log_notes(&self, goal_id: i64, notes: &[Note], evidence: &[String]) -> Result<()> {
        let last: Option<String> = self
            .conn
            .query_row(
                "SELECT notes FROM coach_log WHERE goal_id = ?1 ORDER BY id DESC LIMIT 1",
                [goal_id],
                |r| r.get(0),
            )
            .optional()?;
        let json = serde_json::to_string(notes).unwrap_or_default();
        if last.as_deref() != Some(json.as_str()) && !notes.is_empty() {
            self.conn.execute(
                "INSERT INTO coach_log(at, goal_id, notes, evidence) VALUES(?1, ?2, ?3, ?4)",
                params![
                    now(),
                    goal_id,
                    json,
                    serde_json::to_string(evidence).unwrap_or_default()
                ],
            )?;
        }
        Ok(())
    }

    fn mark_reached(&self, goal_id: i64, on: NaiveDate, by: i64) -> Result<bool> {
        let n = self.conn.execute(
            "UPDATE coach_goal SET achieved_on = ?2, achieved_by = ?3 WHERE id = ?1 AND achieved_on IS NULL",
            params![goal_id, date_str(on), by],
        )?;
        Ok(n > 0)
    }
}

/// Runs only (the coach ignores anything else).
pub fn runs(db: &Db) -> Vec<Activity> {
    db.activities()
        .unwrap_or_default()
        .into_iter()
        .filter(|a| a.modality.is_run())
        .collect()
}

/// What the queue needs from the sessions done so far.
fn context(
    tracked: &[Tracked],
    runs: &[Activity],
    metrics: &std::collections::HashMap<i64, metrics::Metrics>,
    easy_cap: f64,
) -> Context {
    let mut done: Vec<&Tracked> = tracked.iter().filter(|t| t.activity_id.is_some()).collect();
    done.sort_by_key(|t| (t.session.date, t.id));
    let last_run = runs.iter().max_by_key(|r| r.start_time).map(|r| {
        let planned = tracked
            .iter()
            .find(|t| t.activity_id == Some(r.id))
            .map(|t| t.session.kind);
        let above_easy = metrics.get(&r.id).is_some_and(|m| {
            let total: f64 = m.zones_s.iter().sum();
            total > 60.0 && (m.zones_s[3] + m.zones_s[4] + m.zones_s[5]) / total > 0.3
        }) || (metrics.get(&r.id).is_none() && r.avg_hr.is_some_and(|h| h > easy_cap + 5.0));
        let hard = planned.is_some_and(|k| k == SessionKind::Long || k.is_hard()) || above_easy;
        (fitness::day(r), hard)
    });
    Context {
        done: done.len(),
        last_run,
        last_check: done
            .iter()
            .filter(|t| t.session.kind == SessionKind::Check)
            .map(|t| t.session.date)
            .max(),
        since_attempt: done
            .iter()
            .rposition(|t| t.session.kind == SessionKind::GoalAttempt)
            .map(|i| done.len() - 1 - i),
        tapered: done
            .last()
            .is_some_and(|t| t.session.phase == Phase::Taper && t.session.kind == SessionKind::Easy),
    }
}

/// Links runs, reviews the last runs and rebuilds the queue.
/// `goal_changed` adds a note saying so.
pub fn refresh(db: &mut Db, today: NaiveDate, goal_changed: bool) -> Result<Option<CoachState>> {
    let Some(goal) = db.active_goal()? else { return Ok(None) };
    let settings = db.load_settings();
    let zones = settings.zones(db.observed_max_hr());
    let runs = runs(db);
    let metrics = metrics::all_metrics(db);
    let best = db.best_efforts();

    let mut tracked = db.tracked_sessions(goal.id)?;
    for id in adapt::match_runs(&mut tracked, &runs, goal.start_date) {
        if let Some(t) = tracked.iter().find(|t| t.id == id) {
            db.update_tracked(t)?;
        }
    }
    let fitness = fitness::assess(
        &Inputs {
            runs: &runs,
            metrics: &metrics,
            best_efforts: &best,
            zones: &zones,
            today,
        },
        &goal,
    );
    let reached = plan::goal_reached(&goal, &runs, &best);
    let mut adjust = adapt::review(&tracked, &runs, &metrics, today);
    if let Some((on, by)) = reached {
        db.mark_reached(goal.id, on, by)?;
        adjust.notes.insert(0, Note::GoalReached { date: on });
    }
    if goal_changed {
        adjust.notes.insert(0, Note::GoalChanged);
    }
    let ctx = context(&tracked, &runs, &metrics, zones.range(2).1);
    let (queue, projection) = plan::generate(
        &goal,
        &fitness,
        &adjust,
        zones.lower[3] - 1.0,
        today,
        reached.is_some(),
        &ctx,
    );
    if let (Some(avail), false) = (projection.weeks_available, reached.is_some())
        && projection.weeks > avail
    {
        adjust.notes.push(Note::Ambitious {
            weeks_needed: projection.weeks,
            weeks_available: avail,
        });
    }
    // how much sooner with one more run a week
    let per_week = fitness.runs_per_week;
    if reached.is_none() && per_week > 0.0 && per_week < 2.5 {
        let more = (per_week.round() as u32 + 1).clamp(2, 3);
        let sooner = plan::project_at(&goal, &fitness, today, more as f64);
        if sooner.weeks + 2 <= projection.weeks {
            adjust.notes.push(Note::Rhythm {
                per_week,
                weeks: projection.weeks,
                more,
                more_weeks: sooner.weeks,
            });
        }
    }
    db.replace_queue(goal.id, &queue)?;
    db.log_notes(goal.id, &adjust.notes, &adjust.evidence)?;
    let sessions = db.tracked_sessions(goal.id)?;
    Ok(Some(CoachState {
        goal,
        fitness,
        projection,
        adjust,
        sessions,
        reached,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::writer::Synthetic;

    #[test]
    fn goal_versions_sessions_and_refresh() {
        let mut db = Db::open_in_memory().unwrap();
        let today = chrono::Local::now().date_naive();
        assert!(refresh(&mut db, today, false).unwrap().is_none());

        // two recent runs
        for k in [3u32, 6] {
            let start = (today - chrono::Duration::days(k as i64))
                .and_hms_opt(7, 0, 0)
                .unwrap()
                .and_utc()
                .timestamp()
                - crate::fit::activity::FIT_EPOCH;
            let fit = Synthetic::run(start as u32, 1500, 2.6, 140.0, 155.0, true).encode();
            crate::import::import_bytes(&mut db, "r.fit", &fit).unwrap();
        }
        let g = Goal {
            distance_m: 5000.0,
            target_hr: Some(160.0),
            hr_pace: Some(380.0),
            start_date: today - chrono::Duration::days(2),
            ..Default::default()
        };
        let id = db.set_goal(&g).unwrap();
        let st = refresh(&mut db, today, true).unwrap().unwrap();
        assert_eq!(st.goal.id, id);
        assert_eq!(st.queue().len(), plan::QUEUE);
        assert!(st.queue().iter().all(|s| s.session.date >= today));
        assert!(st.adjust.notes.contains(&Note::GoalChanged));
        let n = st.sessions.len();
        // refreshing again is stable
        let st2 = refresh(&mut db, today, false).unwrap().unwrap();
        assert_eq!(st2.sessions.len(), n);
        assert_eq!(
            st2.sessions.iter().map(|s| &s.session).collect::<Vec<_>>(),
            st.sessions.iter().map(|s| &s.session).collect::<Vec<_>>()
        );

        // a run after the goal start fulfils the first session of the queue
        let first = st.queue()[0].session.kind;
        let start = (today - chrono::Duration::days(1))
            .and_hms_opt(7, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp()
            - crate::fit::activity::FIT_EPOCH;
        let fit = Synthetic::run(start as u32, 1500, 2.6, 140.0, 155.0, true).encode();
        crate::import::import_bytes(&mut db, "r2.fit", &fit).unwrap();
        let st = refresh(&mut db, today, false).unwrap().unwrap();
        let done: Vec<&Tracked> = st.sessions.iter().filter(|t| t.activity_id.is_some()).collect();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].session.kind, first);
        assert_eq!(done[0].session.date, today - chrono::Duration::days(1));
        assert_eq!(st.queue().len(), plan::QUEUE);

        // a new goal version replaces the queue and keeps what was done
        let g2 = Goal {
            id,
            distance_m: 8000.0,
            ..st.goal.clone()
        };
        let id2 = db.set_goal(&g2).unwrap();
        let st3 = refresh(&mut db, today, true).unwrap().unwrap();
        assert_eq!(st3.goal.id, id2);
        assert!(db.tracked_sessions(id).unwrap().is_empty());
        assert_eq!(st3.sessions.iter().filter(|t| t.activity_id.is_some()).count(), 1);
        assert_eq!(db.goal_history().unwrap().len(), 2);
        assert!(!db.coach_log(id2, 10).unwrap().is_empty());
    }
}
