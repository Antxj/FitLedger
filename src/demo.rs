//! Fake data for documentation screenshots and manual testing:
//! `FITLEDGER_DEMO=1` fills an empty database with a made-up beginner who
//! sets a goal 8 weeks ago and follows the coach's plan day by day (missing
//! a few sessions). The GPS track is an invented loop: no real place or
//! person is involved.

use crate::coach::store::refresh;
use crate::coach::{Goal, Session, SessionKind, Surface};
use crate::db::Db;
use crate::fit::activity::FIT_EPOCH;
use crate::fit::writer::{Synthetic, SyntheticPoint};
use chrono::{Duration, NaiveDate};

/// Small deterministic pseudo-random generator (same data every time).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % 10_000) as f64 / 10_000.0
    }

    fn noise(&mut self, amp: f64) -> f64 {
        (self.next() - 0.5) * 2.0 * amp
    }
}

const CENTER: (f64, f64) = (45.0, 7.0);
const LOOP_M: f64 = 2000.0;

/// Steady heart rate of the fake runner: fitness from 0 to 1 lowers it ~12 bpm.
fn hr_for(speed_kmh: f64, elapsed_s: f64, fitness: f64, rng: &mut Rng) -> f64 {
    let steady = 110.0 + (speed_kmh - 5.0) * 15.0 - fitness * 12.0;
    let rise = 1.0 - (-elapsed_s / 120.0).exp();
    let drift = elapsed_s / 60.0 * 0.3 * (1.0 - fitness * 0.6);
    90.0 + (steady - 90.0) * rise + drift + rng.noise(1.5)
}

/// Segments (seconds, km/h) → per-second samples.
fn samples(segments: &[(f64, f64)], gps: bool, fitness: f64, rng: &mut Rng) -> Vec<SyntheticPoint> {
    let r = LOOP_M / (2.0 * std::f64::consts::PI);
    let mut out = Vec::new();
    let (mut t, mut dist, mut hr_lag) = (0.0, 0.0, 95.0);
    for &(secs, kmh) in segments {
        for _ in 0..secs as usize {
            let v = (kmh + rng.noise(0.15)) / 3.6;
            dist += v;
            let target = hr_for(kmh, t, fitness, rng);
            hr_lag += (target - hr_lag) * 0.08;
            let theta = dist / LOOP_M * 2.0 * std::f64::consts::PI;
            let pos = gps.then(|| {
                (
                    CENTER.0 + r * theta.cos() / 111_320.0,
                    CENTER.1 + r * theta.sin() / (111_320.0 * CENTER.0.to_radians().cos()),
                )
            });
            let cad = (78.0 + (kmh - 8.0) * 1.6 + rng.noise(1.0)).round() as u8;
            out.push((hr_lag.round() as u8, v, cad, 240.0 + 9.0 * theta.sin(), pos));
            t += 1.0;
        }
    }
    out
}

fn fit_ts(d: NaiveDate, hour: u32) -> u32 {
    // local time UTC-3, as written by the synthetic file
    let local = d.and_hms_opt(hour, 0, 0).unwrap().and_utc().timestamp();
    (local + 3 * 3600 - FIT_EPOCH) as u32
}

/// The fake runner does the session, a bit off the targets like a real person.
fn run_for(s: &Session, fitness: f64, serial: u32, rng: &mut Rng) -> Synthetic {
    let kmh = |pace: (f64, f64)| 3600.0 / ((pace.0 + pace.1) / 2.0) + 0.2;
    let segments: Vec<(f64, f64)> = if s.steps.is_empty() {
        vec![(s.duration_s * (0.95 + rng.next() * 0.1), s.pace.map(kmh).unwrap_or(8.0))]
    } else {
        let mut v = Vec::new();
        for st in &s.steps {
            for _ in 0..st.repeat {
                v.push((st.duration_s, st.pace.map(kmh).unwrap_or(8.0)));
            }
        }
        v
    };
    Synthetic {
        sport: 1,
        sub_sport: if s.treadmill { 1 } else { 0 },
        start: fit_ts(s.date, if s.kind == SessionKind::Long { 8 } else { 19 }),
        serial,
        samples: samples(&segments, !s.treadmill, fitness, rng),
        with_alt: !s.treadmill,
    }
}

pub fn fill(db: &mut Db) -> rusqlite::Result<()> {
    if !db.activities()?.is_empty() {
        return Ok(());
    }
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let today = chrono::Local::now().date_naive();
    let start = today - Duration::days(56);
    let mut s = db.load_settings();
    s.age = Some(36);
    s.hr_rest = Some(62.0);
    s.hr_max = Some(188.0);
    s.onboarded = true;
    db.save_settings(&s)?;
    // two runs before the goal, so the coach has something to start from
    for (k, kmh) in [(9i64, 8.0), (5, 8.2)] {
        let d = start - Duration::days(k);
        let sess = Session {
            date: d,
            kind: SessionKind::Easy,
            phase: crate::coach::Phase::Base,
            week: 0,
            duration_s: 1500.0,
            distance_m: 0.0,
            pace: Some((3600.0 / kmh, 3600.0 / kmh)),
            hr_max: None,
            steps: vec![],
            treadmill: k == 9,
        };
        let fit = run_for(&sess, 0.0, 900 + k as u32, &mut rng);
        let _ = crate::import::import_bytes(db, "first.fit", &fit.encode());
    }
    let goal = Goal {
        distance_m: 5000.0,
        target_hr: Some(160.0),
        hr_pace: Some(390.0),
        days_per_week: 3,
        long_run_day: 6,
        surface: Surface::Any,
        start_date: start,
        ..Default::default()
    };
    db.set_goal(&goal)?;
    let mut serial = 1000;
    let mut d = start;
    while d < today {
        let fitness = (d - start).num_days() as f64 / 70.0;
        if let Some(state) = refresh(db, d, d == start)? {
            for t in state.today(d) {
                // misses about 1 session in 10
                if rng.next() < 0.1 {
                    continue;
                }
                serial += 1;
                let fit = run_for(&t.session, fitness, serial, &mut rng);
                if let Ok(crate::import::Outcome::Imported(id)) =
                    crate::import::import_bytes(db, "run.fit", &fit.encode())
                    && let Ok(Some(mut a)) = db.activity(id)
                {
                    a.rpe = Some(if t.session.kind.is_hard() { 7 } else { 4 });
                    if t.session.kind == SessionKind::Long && rng.next() < 0.3 {
                        a.notes = Some(tr!("Estava quente.", "It was hot.").into());
                    }
                    db.update_activity_user_fields(&a)?;
                }
            }
        }
        d += Duration::days(1);
    }
    refresh(db, today, false)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn demo_runner_follows_the_plan() {
        let mut db = crate::db::Db::open_in_memory().unwrap();
        super::fill(&mut db).unwrap();
        let acts = db.activities().unwrap();
        assert!(acts.len() >= 15, "{}", acts.len());
        let goal = db.active_goal().unwrap().unwrap();
        let sessions = db.tracked_sessions(goal.id).unwrap();
        let done = sessions.iter().filter(|s| s.activity_id.is_some()).count();
        assert!(done >= 15, "{done}");
        assert!(
            sessions
                .iter()
                .any(|s| s.session.date >= chrono::Local::now().date_naive())
        );
        // running a second time does nothing
        super::fill(&mut db).unwrap();
        assert_eq!(db.activities().unwrap().len(), acts.len());
    }
}
