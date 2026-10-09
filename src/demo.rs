//! Fake data for documentation screenshots and manual testing:
//! `FITLEDGER_DEMO=1` fills an empty database with four months of made-up
//! runs, treadmill tests, tennis, strength, plans and goals. The GPS track
//! is an invented loop: no real place or person is involved.

use crate::db::Db;
use crate::fit::activity::FIT_EPOCH;
use crate::fit::writer::{Synthetic, SyntheticPoint};
use crate::model::{Activity, Modality};
use crate::planning::*;
use chrono::{Datelike, Duration, NaiveDate};

/// Small deterministic pseudo-random generator (no dependency, same data every time).
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

/// Fitness from 0 (start) to 1 (today): HR at the same pace drops ~10 bpm.
fn hr_for(speed_kmh: f64, elapsed_s: f64, fitness: f64, rng: &mut Rng) -> f64 {
    let steady = 104.0 + (speed_kmh - 5.0) * 13.5 - fitness * 10.0;
    let rise = 1.0 - (-elapsed_s / 120.0).exp();
    let drift = elapsed_s / 60.0 * 0.25 * (1.0 - fitness * 0.5);
    90.0 + (steady - 90.0) * rise + drift + rng.noise(1.5)
}

/// Segments (seconds, km/h). Returns per-second samples.
fn run_samples(segments: &[(f64, f64)], gps: bool, fitness: f64, rng: &mut Rng) -> Vec<SyntheticPoint> {
    let r = LOOP_M / (2.0 * std::f64::consts::PI);
    let mut out = Vec::new();
    let mut t = 0.0;
    let mut dist = 0.0;
    let mut hr_lag = 95.0;
    for &(secs, kmh) in segments {
        for _ in 0..secs as usize {
            let v = (kmh + rng.noise(0.15)) / 3.6;
            dist += v;
            let target = hr_for(kmh, t, fitness, rng);
            hr_lag += (target - hr_lag) * 0.08;
            let theta = dist / LOOP_M * 2.0 * std::f64::consts::PI;
            let pos = gps.then(|| {
                let dlat = r * theta.cos() / 111_320.0;
                let dlon = r * theta.sin() / (111_320.0 * CENTER.0.to_radians().cos());
                (CENTER.0 + dlat, CENTER.1 + dlon)
            });
            let cad = (78.0 + (kmh - 8.0) * 1.6 + rng.noise(1.0)).round() as u8;
            out.push((hr_lag.round() as u8, v, cad, 240.0 + 9.0 * theta.sin(), pos));
            t += 1.0;
        }
    }
    out
}

fn tennis_samples(minutes: f64, rng: &mut Rng) -> Vec<SyntheticPoint> {
    let mut out = Vec::new();
    let mut hr = 95.0;
    for i in 0..(minutes * 60.0) as usize {
        // points (bursts) and pauses between them
        let burst = (i / 25) % 3 != 2;
        let target = if burst { 158.0 + rng.noise(14.0) } else { 128.0 };
        hr += (target - hr) * 0.06;
        out.push((hr.round() as u8, 0.0, 0, 0.0, None));
    }
    out
}

fn fit_ts(d: NaiveDate, hour: u32, minute: u32) -> u32 {
    // demo activities happen at local time UTC-3 (as written by the synthetic file)
    let local = d.and_hms_opt(hour, minute, 0).unwrap().and_utc().timestamp();
    (local + 3 * 3600 - FIT_EPOCH) as u32
}

fn import(db: &mut Db, name: &str, s: &Synthetic) -> Option<i64> {
    match crate::import::import_bytes(db, name, &s.encode()) {
        Ok(crate::import::Outcome::Imported(id)) => Some(id),
        _ => None,
    }
}

pub fn fill(db: &mut Db) -> rusqlite::Result<()> {
    if !db.activities()?.is_empty() {
        return Ok(());
    }
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let today = chrono::Local::now().date_naive();
    let start = today - Duration::days(119);
    db.ensure_default_exercises()?;
    let exercises = db.exercises()?;
    let ex = |name: &str| exercises.iter().find(|e| e.name == name).map(|e| e.id).unwrap_or(0);
    let test = db.save_reference_test(&RefTest {
        name: tr!("5 km a 6:00/km", "5 km at 6:00/km").into(),
        modality: Modality::Treadmill,
        distance_m: 5000.0,
        target_pace: Some(360.0),
        incline_pct: Some(1.0),
        notes: tr!("Esteira a 10,0 km/h", "Treadmill at 10.0 km/h").into(),
        ..Default::default()
    })?;
    let mut serial = 1000u32;
    let mut long_km = 6.0;
    let mut week = 0;
    let mut d = start;
    while d < today {
        let fitness = (d - start).num_days() as f64 / 120.0;
        serial += 1;
        let wd = d.weekday().num_days_from_monday();
        if wd == 0 {
            week += 1;
        }
        let gps = true;
        match wd {
            0 | 4 => {
                // strength (manual entry)
                let st = d.and_hms_opt(7, 0, 0).unwrap().and_utc().timestamp() + 3 * 3600;
                let a = Activity {
                    modality: Modality::Strength,
                    start_time: st,
                    tz_offset: -3 * 3600,
                    elapsed_s: Some(3300.0),
                    rpe: Some(7),
                    ..Default::default()
                };
                let id = db.save_manual_activity(&a)?;
                let w = |base: f64, step: f64| Some(base + step * (week / 2) as f64);
                let groups = if wd == 0 {
                    vec![
                        SetGroup {
                            exercise_id: ex(tr!("Supino", "Bench press")),
                            sets: 3,
                            reps: Some(10),
                            weight_kg: w(50.0, 2.5),
                            rpe: Some(8.0),
                            rest_s: Some(90.0),
                            ..Default::default()
                        },
                        SetGroup {
                            exercise_id: ex(tr!("Remada", "Row")),
                            sets: 3,
                            reps: Some(12),
                            weight_kg: w(40.0, 2.5),
                            ..Default::default()
                        },
                        SetGroup {
                            exercise_id: ex(tr!("Desenvolvimento", "Shoulder press")),
                            sets: 3,
                            reps: Some(10),
                            weight_kg: w(14.0, 1.0),
                            ..Default::default()
                        },
                    ]
                } else {
                    vec![
                        SetGroup {
                            exercise_id: ex(tr!("Agachamento", "Squat")),
                            sets: 4,
                            reps: Some(8),
                            weight_kg: w(60.0, 5.0),
                            rpe: Some(8.0),
                            ..Default::default()
                        },
                        SetGroup {
                            exercise_id: ex(tr!("Leg press", "Leg press")),
                            sets: 3,
                            reps: Some(12),
                            weight_kg: w(120.0, 10.0),
                            ..Default::default()
                        },
                        SetGroup {
                            exercise_id: ex(tr!("Rosca", "Biceps curl")),
                            sets: 3,
                            reps: Some(12),
                            weight_kg: w(10.0, 1.0),
                            ..Default::default()
                        },
                    ]
                };
                let groups: Vec<SetGroup> = groups
                    .into_iter()
                    .map(|mut g| {
                        g.activity_id = id;
                        g
                    })
                    .collect();
                db.save_set_groups(id, &groups)?;
            }
            1 => {
                let kmh = 9.0 + rng.noise(0.3);
                let s = Synthetic {
                    sport: 1,
                    sub_sport: 0,
                    start: fit_ts(d, 6, 40),
                    serial,
                    samples: run_samples(&[(5.5 * 3600.0 / kmh, kmh)], gps, fitness, &mut rng),
                    with_alt: true,
                };
                import(db, "easy.fit", &s);
            }
            2 if week % 3 == 0 => {
                let s = Synthetic {
                    sport: 1,
                    sub_sport: 1,
                    start: fit_ts(d, 19, 30),
                    serial,
                    samples: run_samples(&[(30.0 * 60.0, 10.0)], false, fitness, &mut rng),
                    with_alt: false,
                };
                if let Some(id) = import(db, "test.fit", &s) {
                    db.set_reference_run(test, id, true)?;
                    if let Ok(Some(mut a)) = db.activity(id) {
                        a.incline_pct = Some(1.0);
                        a.name = Some(tr!("Teste 5 km", "5 km test").into());
                        db.update_activity_user_fields(&a)?;
                    }
                }
            }
            3 => {
                let mut segs = vec![(600.0, 8.6)];
                for _ in 0..5 {
                    segs.push((180.0, 10.9));
                    segs.push((120.0, 8.0));
                }
                segs.push((600.0, 8.4));
                let s = Synthetic {
                    sport: 1,
                    sub_sport: 0,
                    start: fit_ts(d, 6, 30),
                    serial,
                    samples: run_samples(&segs, gps, fitness, &mut rng),
                    with_alt: true,
                };
                if let Some(id) = import(db, "intervals.fit", &s)
                    && let Ok(Some(mut a)) = db.activity(id)
                {
                    a.name = Some(tr!("Intervalado 5 × 3 min", "Intervals 5 × 3 min").into());
                    a.rpe = Some(8);
                    if week % 4 == 1 {
                        a.notes = Some(tr!("Estava muito calor.", "It was very hot.").into());
                    }
                    db.update_activity_user_fields(&a)?;
                }
            }
            5 => {
                let s = Synthetic {
                    sport: 8,
                    sub_sport: 0,
                    start: fit_ts(d, 9, 0),
                    serial,
                    samples: tennis_samples(70.0 + rng.noise(20.0), &mut rng),
                    with_alt: false,
                };
                import(db, "tennis.fit", &s);
            }
            6 => {
                let kmh = 8.9;
                long_km = (long_km + 0.35_f64).min(10.0);
                let s = Synthetic {
                    sport: 1,
                    sub_sport: 0,
                    start: fit_ts(d, 7, 0),
                    serial,
                    samples: run_samples(&[(long_km * 3600.0 / kmh, kmh)], gps, fitness, &mut rng),
                    with_alt: true,
                };
                if let Some(id) = import(db, "long.fit", &s)
                    && let Ok(Some(mut a)) = db.activity(id)
                {
                    a.name = Some(tr!("Longão", "Long run").into());
                    if week % 5 == 2 {
                        a.notes = Some(tr!("Joguei tênis no dia anterior.", "Played tennis the day before.").into());
                        a.sleep_quality = Some(3);
                        a.fatigue = Some(3);
                    }
                    db.update_activity_user_fields(&a)?;
                }
            }
            _ => {}
        }
        d += Duration::days(1);
    }

    // plan, planned workouts for this week and the next one
    let ws = crate::analysis::load::week_start(today);
    let plan = db.save_plan(&Plan {
        name: tr!("Melhorar eficiência nos 5 km", "Improve 5 km efficiency").into(),
        goal: tr!("Correr 5 km a 6:00/km com menor frequência cardíaca", "Run 5 km at 6:00/km with a lower heart rate").into(),
        objectives: tr!(
            "Correr 5 km a 6:00/km com menor frequência cardíaca\nAumentar corrida longa até 10 km\nManter tênis\nManter musculação",
            "Run 5 km at 6:00/km with a lower heart rate\nIncrease the long run up to 10 km\nKeep tennis\nKeep strength training"
        )
        .into(),
        start_date: Some(start),
        end_date: Some(ws + Duration::days(27)),
        sessions_per_week: Some(6),
        modalities: vec![Modality::RoadRun, Modality::Treadmill, Modality::Tennis, Modality::Strength],
        ..Default::default()
    })?;
    let acts = db.activities()?;
    for k in 0..14 {
        let day = ws + Duration::days(k);
        let wd = day.weekday().num_days_from_monday();
        let mut w = PlannedWorkout::new(day);
        w.plan_id = Some(plan);
        match wd {
            0 | 4 => {
                w.modality = Modality::Strength;
                w.run_type = None;
            }
            1 => {
                w.run_type = Some(RunType::Easy);
                w.distance_m = Some(5500.0);
                w.target_pace = Some(400.0);
                w.target_zone = Some(2);
            }
            3 => {
                w.run_type = Some(RunType::Intervals);
                w.steps = vec![
                    Step::Block {
                        kind: StepKind::Warmup,
                        duration_s: Some(600.0),
                        distance_m: None,
                        pace: Some(420.0),
                        hr_zone: Some(2),
                        note: String::new(),
                    },
                    Step::Repeat {
                        times: 5,
                        steps: vec![
                            Step::Block {
                                kind: StepKind::Work,
                                duration_s: Some(180.0),
                                distance_m: None,
                                pace: Some(330.0),
                                hr_zone: Some(4),
                                note: String::new(),
                            },
                            Step::Block {
                                kind: StepKind::Recovery,
                                duration_s: Some(120.0),
                                distance_m: None,
                                pace: Some(450.0),
                                hr_zone: Some(2),
                                note: String::new(),
                            },
                        ],
                    },
                    Step::Block {
                        kind: StepKind::Cooldown,
                        duration_s: Some(600.0),
                        distance_m: None,
                        pace: None,
                        hr_zone: Some(1),
                        note: String::new(),
                    },
                ];
            }
            5 => {
                w.modality = Modality::Tennis;
                w.run_type = None;
                w.duration_s = Some(5400.0);
            }
            6 => {
                w.run_type = Some(RunType::Long);
                w.distance_m = Some(10_000.0);
                w.target_pace = Some(405.0);
            }
            _ => continue,
        }
        if day < today {
            w.activity_id = acts
                .iter()
                .find(|a| {
                    crate::analysis::load::day_of(a) == day
                        && (a.modality == w.modality || (a.modality.is_run() && w.modality.is_run()))
                })
                .map(|a| a.id);
        }
        db.save_planned_workout(&w)?;
    }

    // goals and a long-term objective
    let goals = [
        (
            GoalKind::Distance,
            tr!("Correr 100 km no mês", "Run 100 km a month"),
            GoalParams {
                distance_m: Some(100_000.0),
                period: GoalPeriod::Month,
                ..Default::default()
            },
        ),
        (
            GoalKind::Frequency,
            tr!("Correr 3 vezes por semana", "Run 3 times a week"),
            GoalParams {
                count: Some(3.0),
                ..Default::default()
            },
        ),
        (
            GoalKind::Performance,
            tr!("5 km abaixo de 28 minutos", "5 km under 28 minutes"),
            GoalParams {
                distance_m: Some(5000.0),
                time_s: Some(28.0 * 60.0),
                ..Default::default()
            },
        ),
        (
            GoalKind::HeartRate,
            tr!(
                "5 km a 6:00/km com FC média abaixo de 165 bpm",
                "5 km at 6:00/km with avg HR below 165 bpm"
            ),
            GoalParams {
                distance_m: Some(5000.0),
                pace: Some(360.0),
                hr: Some(165.0),
                ..Default::default()
            },
        ),
        (
            GoalKind::Frequency,
            tr!("Musculação 3 vezes por semana", "Strength 3 times a week"),
            GoalParams {
                count: Some(3.0),
                modality: Some("strength".into()),
                ..Default::default()
            },
        ),
        (
            GoalKind::Hours,
            tr!("Jogar 4 horas de tênis no mês", "Play 4 hours of tennis a month"),
            GoalParams {
                hours: Some(4.0),
                modality: Some("tennis".into()),
                period: GoalPeriod::Month,
                ..Default::default()
            },
        ),
    ];
    for (i, (kind, title, params)) in goals.into_iter().enumerate() {
        db.save_goal(&Goal {
            kind,
            title: title.into(),
            params,
            position: i as i64,
            ..Default::default()
        })?;
    }
    let obj = db.save_objective(&Objective {
        title: tr!("Correr 10 km confortavelmente", "Run 10 km comfortably").into(),
        target_date: Some(today + Duration::days(90)),
        ..Default::default()
    })?;
    for (i, km) in (5..=10).enumerate() {
        db.save_goal(&Goal {
            kind: GoalKind::MaxDistance,
            title: format!("{km} km"),
            objective_id: Some(obj),
            position: i as i64,
            params: GoalParams {
                distance_m: Some(km as f64 * 1000.0),
                period: GoalPeriod::Range,
                ..Default::default()
            },
            ..Default::default()
        })?;
    }
    let mut s = db.load_settings();
    s.age = Some(38);
    s.hr_rest = Some(58.0);
    s.hr_max = Some(190.0);
    s.onboarded = true;
    db.save_settings(&s)?;
    crate::analysis::metrics::recalculate_all(db, &mut |_, _| {})?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn demo_data_fills_every_area() {
        let mut db = crate::db::Db::open_in_memory().unwrap();
        super::fill(&mut db).unwrap();
        let acts = db.activities().unwrap();
        for m in crate::model::Modality::ALL
            .iter()
            .filter(|m| **m != crate::model::Modality::Other)
        {
            assert!(acts.iter().any(|a| a.modality == *m), "{m:?}");
        }
        assert!(db.reference_runs(1).unwrap().len() >= 4);
        assert_eq!(db.goals().unwrap().len(), 12);
        // running a second time does nothing
        super::fill(&mut db).unwrap();
        assert_eq!(db.activities().unwrap().len(), acts.len());
    }
}
