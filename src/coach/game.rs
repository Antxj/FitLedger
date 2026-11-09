//! Feedback that rewards **following the plan**: each session gets up to
//! three stars (done at the planned size, at the planned effort, on target),
//! plus a weekly streak and achievements. Running more or harder than
//! planned earns nothing extra (and can cost a star).
//!
//! Everything is derived from the runs and the plan, so it can always be
//! recalculated and never drifts from the data.

use super::adapt::Tracked;
use super::fitness::day;
use super::{Phase, SessionKind, Status, week_start};
use crate::analysis::metrics::Metrics;
use crate::format;
use crate::model::{Activity, Modality};
use chrono::{Duration, NaiveDate, Timelike};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Result of one planned session that has a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub session_id: i64,
    pub activity_id: i64,
    /// Did the session: about the planned time (90% to 130%).
    pub star_done: bool,
    /// Stayed at the planned effort (heart-rate cap / pace range).
    pub star_effort: bool,
    /// Hit the session's own target (check distance, intervals, goal...).
    pub star_target: bool,
    /// Why each star was or was not given.
    pub reasons: Vec<String>,
}

impl Score {
    pub fn stars(&self) -> u8 {
        self.star_done as u8 + self.star_effort as u8 + self.star_target as u8
    }
}

/// Share of the run spent above zone 2 (stored zone times).
fn share_above_z2(m: &Metrics) -> Option<f64> {
    let total: f64 = m.zones_s.iter().sum();
    (total > 60.0).then(|| (m.zones_s[3] + m.zones_s[4] + m.zones_s[5]) / total)
}

pub fn score(t: &Tracked, a: &Activity, m: Option<&Metrics>, goal_reached: bool) -> Score {
    let s = &t.session;
    let mut reasons = Vec::new();
    let planned = s.duration_s.max(1.0);
    let done = a.duration().unwrap_or(0.0);
    let ratio = done / planned;
    let dist_ok = a.distance().unwrap_or(0.0) >= s.distance_m * 0.9;
    let star_done =
        t.status == Status::Done && (0.9..=1.3).contains(&ratio) || (s.kind == SessionKind::GoalAttempt && dist_ok);
    reasons.push(if star_done {
        trf!(
            "★ Treino feito: {} de {} planejados",
            "★ Session done: {} of {} planned",
            format::duration(done),
            format::duration(planned)
        )
    } else if ratio > 1.3 {
        trf!(
            "Mais longo que o planejado ({} de {}): seguir o plano rende mais que correr além dele",
            "Longer than planned ({} of {}): following the plan pays more than going beyond it",
            format::duration(done),
            format::duration(planned)
        )
    } else {
        trf!(
            "Mais curto que o planejado ({} de {})",
            "Shorter than planned ({} of {})",
            format::duration(done),
            format::duration(planned)
        )
    });

    let star_effort = match s.kind {
        SessionKind::Easy | SessionKind::Long => {
            let share = m.and_then(share_above_z2);
            let ok = share.is_some_and(|x| x <= 0.10);
            reasons.push(match share {
                Some(x) if ok => trf!(
                    "★ Esforço certo: só {}% do tempo acima do limite de FC",
                    "★ Right effort: only {}% of the time above the HR limit",
                    format::num(x * 100.0, 0)
                ),
                Some(x) => trf!(
                    "{}% do tempo acima do limite de FC: na próxima, mais devagar",
                    "{}% of the time above the HR limit: slower next time",
                    format::num(x * 100.0, 0)
                ),
                None => tr!(
                    "Sem FC registrada: não dá para conferir o esforço",
                    "No HR recorded: effort cannot be checked"
                )
                .into(),
            });
            ok
        }
        _ => {
            // harder sessions: average pace inside (or near) the planned range
            let ok = match (s.pace, a.pace_s_per_km()) {
                (Some((fast, slow)), Some(p)) => {
                    // the run includes warm-up/cool-down: allow the slow side generously
                    p >= fast - 15.0 && p <= slow + 75.0
                }
                _ => false,
            };
            reasons.push(if ok {
                tr!("★ Ritmo dentro do proposto", "★ Pace as proposed").into()
            } else {
                tr!("Ritmo fora do proposto para a sessão", "Pace off the session's range").into()
            });
            ok
        }
    };

    let star_target = match s.kind {
        SessionKind::Easy | SessionKind::Long => {
            // easy/long: the target is not overdoing it (no more than 130% of the time)
            let ok = ratio <= 1.3 && t.status == Status::Done;
            reasons.push(if ok {
                tr!(
                    "★ Sem exagerar: respeitou o tamanho do treino",
                    "★ No overdoing: kept to the session's size"
                )
                .into()
            } else {
                tr!(
                    "Objetivo da sessão: fazer o tamanho planejado, sem exagerar",
                    "Session target: the planned size, without overdoing it"
                )
                .into()
            });
            ok
        }
        SessionKind::Check => {
            let ok = dist_ok;
            reasons.push(if ok {
                tr!(
                    "★ Teste completo: FC registrada no pace da meta",
                    "★ Check complete: HR recorded at goal pace"
                )
                .into()
            } else {
                tr!(
                    "Teste incompleto: faltou distância no pace da meta",
                    "Check incomplete: not enough distance at goal pace"
                )
                .into()
            });
            ok
        }
        SessionKind::GoalAttempt => {
            reasons.push(if goal_reached {
                tr!("★ Meta atingida!", "★ Goal reached!").into()
            } else {
                tr!(
                    "A meta ainda não foi atingida nesta tentativa",
                    "The goal was not reached on this attempt"
                )
                .into()
            });
            goal_reached
        }
        SessionKind::Tempo | SessionKind::Intervals => {
            let ok = t.status == Status::Done && ratio >= 0.9;
            reasons.push(if ok {
                tr!("★ Todos os blocos feitos", "★ All blocks done").into()
            } else {
                tr!("Blocos incompletos", "Blocks incomplete").into()
            });
            ok
        }
    };

    Score {
        session_id: t.id,
        activity_id: a.id,
        star_done,
        star_effort,
        star_target,
        reasons,
    }
}

/// One week for the streak.
#[derive(Debug, Clone, PartialEq)]
pub struct WeekResult {
    pub start: NaiveDate,
    /// Plan sessions done.
    pub done: usize,
    /// None = not counted (sick/travel).
    pub consistent: Option<bool>,
    pub joker_used: bool,
    /// Two or more plan sessions (gold medal).
    pub two: bool,
    /// Two or more plan sessions, all with 3 stars.
    pub perfect: bool,
}

/// There are no fixed days, so a week counts when at least one plan session
/// was done. Weeks go from the first plan session to now; the current week
/// only shows once it counts. One "joker" per 8 weeks forgives a week.
pub fn weeks(
    sessions: &[Tracked],
    scores: &HashMap<i64, Score>,
    sick: &HashSet<NaiveDate>,
    today: NaiveDate,
) -> Vec<WeekResult> {
    let done: Vec<&Tracked> = sessions.iter().filter(|t| t.activity_id.is_some()).collect();
    let Some(first) = done.iter().map(|t| t.session.date).min() else {
        return Vec::new();
    };
    let current = week_start(today);
    let mut out = Vec::new();
    let mut ws = week_start(first);
    while ws <= current {
        let list: Vec<&&Tracked> = done.iter().filter(|t| week_start(t.session.date) == ws).collect();
        let n = list.len();
        if ws == current && n == 0 {
            break;
        }
        out.push(WeekResult {
            start: ws,
            done: n,
            consistent: if sick.contains(&ws) { None } else { Some(n >= 1) },
            joker_used: false,
            two: n >= 2,
            perfect: n >= 2 && list.iter().all(|t| scores.get(&t.id).is_some_and(|s| s.stars() == 3)),
        });
        ws += Duration::days(7);
    }
    out
}

/// Current streak of consistent weeks (with jokers), and the best one.
pub fn streak(weeks: &mut [WeekResult]) -> (u32, u32) {
    let (mut cur, mut best, mut since_joker) = (0u32, 0u32, 8u32);
    for w in weeks.iter_mut() {
        match w.consistent {
            None => {}
            Some(true) => {
                cur += 1;
                since_joker += 1;
            }
            Some(false) if since_joker >= 8 && cur > 0 => {
                w.joker_used = true;
                since_joker = 0;
            }
            Some(false) => {
                cur = 0;
            }
        }
        best = best.max(cur);
    }
    (cur, best)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Achievement {
    pub id: &'static str,
    pub icon: &'static str,
    pub title: String,
    pub how: String,
    pub unlocked: Option<NaiveDate>,
    /// Progress toward it, e.g. (3, 5).
    pub progress: Option<(u32, u32)>,
    /// Big ones get a celebration.
    pub major: bool,
}

pub struct Inputs<'a> {
    pub sessions: &'a [Tracked],
    pub runs: &'a [Activity],
    pub metrics: &'a HashMap<i64, Metrics>,
    /// (date, estimated HR at goal pace) per run.
    pub hr_history: &'a [(NaiveDate, f64)],
    pub goal_reached: Option<NaiveDate>,
    pub has_goal: bool,
    pub sick_weeks: &'a HashSet<NaiveDate>,
    pub today: NaiveDate,
}

/// Everything the game shows.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub scores: HashMap<i64, Score>,
    pub weeks: Vec<WeekResult>,
    pub streak: u32,
    pub best_streak: u32,
    pub achievements: Vec<Achievement>,
}

#[allow(clippy::too_many_arguments)]
fn ach(
    id: &'static str,
    icon: &'static str,
    pt: &str,
    en: &str,
    how_pt: &str,
    how_en: &str,
    unlocked: Option<NaiveDate>,
    progress: Option<(u32, u32)>,
    major: bool,
) -> Achievement {
    Achievement {
        id,
        icon,
        title: if crate::i18n::pt() { pt.into() } else { en.into() },
        how: if crate::i18n::pt() {
            how_pt.into()
        } else {
            how_en.into()
        },
        unlocked,
        progress,
        major,
    }
}

/// Date when the n-th item (sorted dates) happened.
fn nth(dates: &[NaiveDate], n: usize) -> Option<NaiveDate> {
    (n >= 1 && dates.len() >= n).then(|| dates[n - 1])
}

pub fn compute(inp: &Inputs) -> State {
    let by_id: HashMap<i64, &Activity> = inp.runs.iter().map(|a| (a.id, a)).collect();
    let mut scores = HashMap::new();
    for t in inp.sessions {
        if let Some(a) = t.activity_id.and_then(|id| by_id.get(&id)) {
            let reached = inp.goal_reached.is_some_and(|d| d == day(a));
            scores.insert(t.id, score(t, a, inp.metrics.get(&a.id), reached));
        }
    }
    let mut weeks = weeks(inp.sessions, &scores, inp.sick_weeks, inp.today);
    let (streak, best_streak) = streak(&mut weeks);

    // ---- achievements
    let mut runs: Vec<&Activity> = inp.runs.iter().collect();
    runs.sort_by_key(|a| a.start_time);
    let run_dates: Vec<NaiveDate> = runs.iter().map(|a| day(a)).collect();
    let dist = |km: f64| {
        runs.iter()
            .find(|a| a.distance().unwrap_or(0.0) >= km * 980.0)
            .map(|a| day(a))
    };
    let mut scored: Vec<(&Tracked, &Score)> = inp
        .sessions
        .iter()
        .filter_map(|t| Some((t, scores.get(&t.id)?)))
        .collect();
    scored.sort_by_key(|(t, _)| t.session.date);
    let easy_effort: Vec<NaiveDate> = scored
        .iter()
        .filter(|(t, s)| matches!(t.session.kind, SessionKind::Easy | SessionKind::Long) && s.star_effort)
        .map(|(t, _)| t.session.date)
        .collect();
    let three_stars: Vec<NaiveDate> = scored
        .iter()
        .filter(|(_, s)| s.stars() == 3)
        .map(|(t, _)| t.session.date)
        .collect();
    let checks: Vec<NaiveDate> = scored
        .iter()
        .filter(|(t, s)| t.session.kind == SessionKind::Check && s.star_target)
        .map(|(t, _)| t.session.date)
        .collect();
    let consistent_weeks: Vec<NaiveDate> = {
        // date when each streak length was first reached
        let (mut cur, mut out) = (0usize, Vec::new());
        for w in &weeks {
            match w.consistent {
                Some(true) => {
                    cur += 1;
                    if cur > out.len() {
                        out.push(w.start + Duration::days(6));
                    }
                }
                Some(false) if !w.joker_used => cur = 0,
                _ => {}
            }
        }
        out
    };
    let first_week = weeks.iter().find(|w| w.two).map(|w| w.start + Duration::days(6));
    let perfect_week = weeks.iter().find(|w| w.perfect).map(|w| w.start + Duration::days(6));
    let light_week = scored
        .iter()
        .find(|(t, s)| t.session.phase == Phase::Light && s.star_effort && s.star_target)
        .map(|(t, _)| t.session.date);
    let low_drift = scored
        .iter()
        .filter(|(t, _)| t.session.kind == SessionKind::Long)
        .find(|(t, _)| {
            t.activity_id
                .and_then(|id| inp.metrics.get(&id))
                .and_then(|m| m.drift.as_ref())
                .and_then(|d| d.decoupling_pct)
                .is_some_and(|d| d < 5.0)
        })
        .map(|(t, _)| t.session.date);
    let early: Vec<NaiveDate> = runs
        .iter()
        .filter(|a| a.local_start().hour() < 7)
        .map(|a| day(a))
        .collect();
    let both_surfaces = {
        let mut weeks_seen: BTreeMap<NaiveDate, (bool, bool)> = BTreeMap::new();
        let mut found = None;
        for a in &runs {
            let e = weeks_seen.entry(week_start(day(a))).or_default();
            if a.modality == Modality::RoadRun {
                e.0 = true;
            } else if a.modality == Modality::Treadmill {
                e.1 = true;
            }
            if e.0 && e.1 && found.is_none() {
                found = Some(day(a));
            }
        }
        found
    };
    // heart efficiency: drop from the average of the first 3 points to a later point
    let heart = |bpm: f64| -> (Option<NaiveDate>, u32) {
        let h = inp.hr_history;
        if h.len() < 4 {
            return (None, 0);
        }
        let base = h[..3].iter().map(|x| x.1).sum::<f64>() / 3.0;
        let best = h[3..].iter().map(|x| base - x.1).fold(0.0, f64::max);
        let at = h[3..].iter().find(|x| base - x.1 >= bpm).map(|x| x.0);
        (at, best.max(0.0).round() as u32)
    };
    let (h3, drop) = heart(3.0);
    let (h5, _) = heart(5.0);
    let (h10, _) = heart(10.0);
    let first_session = scored.first().map(|(t, _)| t.session.date);
    let cnt = |v: &[NaiveDate], target: u32| Some(((v.len() as u32).min(target), target));

    let achievements = vec![
        ach(
            "first_run",
            "👟",
            "Primeira corrida",
            "First run",
            "Importe sua primeira corrida.",
            "Import your first run.",
            run_dates.first().copied(),
            None,
            false,
        ),
        ach(
            "first_plan",
            "📋",
            "No plano",
            "On the plan",
            "Faça o primeiro treino proposto pelo plano.",
            "Do the first session proposed by the plan.",
            first_session.filter(|_| inp.has_goal),
            None,
            false,
        ),
        ach(
            "three_stars",
            "★",
            "Treino perfeito",
            "Perfect session",
            "Ganhe as 3 estrelas em um treino: feito, no esforço certo e no objetivo.",
            "Earn all 3 stars in a session: done, right effort and on target.",
            three_stars.first().copied(),
            None,
            false,
        ),
        ach(
            "first_week",
            "📅",
            "Duas na semana",
            "Two in a week",
            "Faça 2 treinos do plano na mesma semana.",
            "Do 2 plan sessions in the same week.",
            first_week,
            None,
            false,
        ),
        ach(
            "perfect_week",
            "🌟",
            "Semana perfeita",
            "Perfect week",
            "2 ou mais treinos numa semana, todos com 3 estrelas.",
            "2 or more sessions in a week, all with 3 stars.",
            perfect_week,
            None,
            true,
        ),
        ach(
            "streak_4",
            "🔥",
            "4 semanas seguidas",
            "4-week streak",
            "4 semanas seguidas com pelo menos 1 treino do plano.",
            "4 weeks in a row with at least 1 plan session.",
            nth(&consistent_weeks, 4),
            Some(((consistent_weeks.len() as u32).min(4), 4)),
            true,
        ),
        ach(
            "streak_8",
            "🔥",
            "8 semanas seguidas",
            "8-week streak",
            "8 semanas seguidas com pelo menos 1 treino do plano.",
            "8 weeks in a row with at least 1 plan session.",
            nth(&consistent_weeks, 8),
            Some(((consistent_weeks.len() as u32).min(8), 8)),
            true,
        ),
        ach(
            "streak_12",
            "🔥",
            "12 semanas seguidas",
            "12-week streak",
            "12 semanas seguidas com pelo menos 1 treino do plano.",
            "12 weeks in a row with at least 1 plan session.",
            nth(&consistent_weeks, 12),
            Some(((consistent_weeks.len() as u32).min(12), 12)),
            true,
        ),
        ach(
            "zone2",
            "💚",
            "Zona 2 de verdade",
            "True zone 2",
            "5 corridas leves do plano no esforço certo (FC dentro do limite).",
            "5 easy runs of the plan at the right effort (HR within the limit).",
            nth(&easy_effort, 5),
            cnt(&easy_effort, 5),
            false,
        ),
        ach(
            "light_week",
            "🌙",
            "Respeitou a fase leve",
            "Took the light phase",
            "Faça um treino da fase leve do plano sem exagerar (esforço e tamanho certos).",
            "Do a session of the plan's light phase without overdoing it (right effort and size).",
            light_week,
            None,
            false,
        ),
        ach(
            "check",
            "⏱",
            "Teste de evolução",
            "Progress check",
            "Complete um teste de evolução no pace da meta.",
            "Complete a progress check at goal pace.",
            checks.first().copied(),
            None,
            false,
        ),
        ach(
            "drift",
            "📉",
            "Deriva sob controle",
            "Drift under control",
            "Um longão do plano com deriva cardíaca abaixo de 5%.",
            "A long run of the plan with cardiac drift below 5%.",
            low_drift,
            None,
            false,
        ),
        ach(
            "heart_3",
            "❤",
            "Coração mais eficiente",
            "More efficient heart",
            "FC no pace da meta 3 bpm menor que no começo.",
            "HR at goal pace 3 bpm lower than at the start.",
            h3,
            Some((drop.min(3), 3)),
            false,
        ),
        ach(
            "heart_5",
            "❤",
            "Coração eficiente",
            "Efficient heart",
            "FC no pace da meta 5 bpm menor que no começo.",
            "HR at goal pace 5 bpm lower than at the start.",
            h5,
            Some((drop.min(5), 5)),
            true,
        ),
        ach(
            "heart_10",
            "❤",
            "Coração de fundista",
            "Distance runner's heart",
            "FC no pace da meta 10 bpm menor que no começo.",
            "HR at goal pace 10 bpm lower than at the start.",
            h10,
            Some((drop.min(10), 10)),
            true,
        ),
        ach(
            "km3",
            "🏁",
            "3 km",
            "3 km",
            "Corra 3 km sem parar.",
            "Run 3 km non-stop.",
            dist(3.0),
            None,
            false,
        ),
        ach(
            "km5",
            "🏁",
            "5 km",
            "5 km",
            "Corra 5 km sem parar.",
            "Run 5 km non-stop.",
            dist(5.0),
            None,
            true,
        ),
        ach(
            "km8",
            "🏁",
            "8 km",
            "8 km",
            "Corra 8 km sem parar.",
            "Run 8 km non-stop.",
            dist(8.0),
            None,
            false,
        ),
        ach(
            "km10",
            "🏁",
            "10 km",
            "10 km",
            "Corra 10 km sem parar.",
            "Run 10 km non-stop.",
            dist(10.0),
            None,
            true,
        ),
        ach(
            "goal",
            "🏆",
            "Meta atingida",
            "Goal reached",
            "Atinja a sua meta.",
            "Reach your goal.",
            inp.goal_reached,
            None,
            true,
        ),
        ach(
            "early",
            "☀",
            "Madrugador",
            "Early bird",
            "5 corridas começando antes das 7h.",
            "5 runs starting before 7 am.",
            nth(&early, 5),
            cnt(&early, 5),
            false,
        ),
        ach(
            "surfaces",
            "🔁",
            "Rua e esteira",
            "Road and treadmill",
            "Corra na rua e na esteira na mesma semana.",
            "Run on the road and on the treadmill in the same week.",
            both_surfaces,
            None,
            false,
        ),
    ];
    State {
        scores,
        weeks,
        streak,
        best_streak,
        achievements,
    }
}

/// Week starts marked as sick/travel (not counted for the streak or by the coach).
pub fn load_sick_weeks(db: &crate::db::Db) -> HashSet<NaiveDate> {
    db.get_kv("sick_weeks")
        .and_then(|s| serde_json::from_str::<Vec<NaiveDate>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

pub fn toggle_sick_week(db: &crate::db::Db, week: NaiveDate) {
    let mut set = load_sick_weeks(db);
    if !set.remove(&week) {
        set.insert(week);
    }
    let mut v: Vec<NaiveDate> = set.into_iter().collect();
    v.sort();
    let _ = db.set_kv("sick_weeks", &serde_json::to_string(&v).unwrap_or_default());
}

/// Achievements already shown to the user (so a celebration plays once).
pub fn load_seen(db: &crate::db::Db) -> HashSet<String> {
    db.get_kv("game_seen")
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

pub fn save_seen(db: &crate::db::Db, seen: &HashSet<String>) {
    let mut v: Vec<&String> = seen.iter().collect();
    v.sort();
    let _ = db.set_kv("game_seen", &serde_json::to_string(&v).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coach::Session;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn sess(id: i64, date: &str, kind: SessionKind, mins: f64, status: Status, run: Option<i64>, week: u32) -> Tracked {
        Tracked {
            id,
            session: Session {
                date: d(date),
                kind,
                phase: Phase::Base,
                week,
                duration_s: mins * 60.0,
                distance_m: mins * 60.0 / 420.0 * 1000.0,
                pace: Some((420.0, 465.0)),
                hr_max: Some(149.0),
                steps: vec![],
                treadmill: false,
            },
            status,
            activity_id: run,
        }
    }

    fn run(id: i64, date: &str, mins: f64) -> Activity {
        Activity {
            id,
            start_time: d(date).and_hms_opt(8, 0, 0).unwrap().and_utc().timestamp(),
            modality: Modality::RoadRun,
            distance_m: Some(mins * 60.0 / 430.0 * 1000.0),
            moving_s: Some(mins * 60.0),
            avg_hr: Some(140.0),
            ..Default::default()
        }
    }

    fn easy_metrics(share_above: f64) -> Metrics {
        Metrics {
            zones_s: [0.0, 100.0, 1000.0 * (1.0 - share_above), 1000.0 * share_above, 0.0, 0.0],
            ..Default::default()
        }
    }

    #[test]
    fn following_the_plan_scores_more_than_overdoing_it() {
        let t = sess(1, "2026-10-20", SessionKind::Easy, 30.0, Status::Done, Some(10), 1);
        let good = score(&t, &run(10, "2026-10-20", 31.0), Some(&easy_metrics(0.05)), false);
        assert_eq!(good.stars(), 3);
        // twice as long and too hard: no extra reward for doing more
        let over = score(&t, &run(10, "2026-10-20", 60.0), Some(&easy_metrics(0.40)), false);
        assert!(over.stars() < good.stars());
        assert!(over.reasons.iter().any(|r| r.contains("plano") || r.contains("plan")));
    }

    #[test]
    fn streak_with_joker_and_sick_weeks() {
        let mk = |c: Option<bool>| WeekResult {
            start: d("2026-01-05"),
            done: 0,
            consistent: c,
            joker_used: false,
            two: false,
            perfect: false,
        };
        let mut w: Vec<WeekResult> = [
            Some(true),
            Some(true),
            Some(false),
            Some(true),
            None,
            Some(true),
            Some(false),
        ]
        .into_iter()
        .map(mk)
        .collect();
        let (cur, best) = streak(&mut w);
        // the first bad week uses the joker; the sick week is neutral; the second
        // bad week resets
        assert!(w[2].joker_used);
        assert!(!w[6].joker_used);
        assert_eq!(cur, 0);
        assert_eq!(best, 4);
    }

    #[test]
    fn a_week_without_runs_breaks_the_streak_unless_marked() {
        let today = d("2026-11-09");
        let sessions = vec![
            sess(1, "2026-10-20", SessionKind::Easy, 30.0, Status::Done, Some(10), 1),
            sess(2, "2026-11-03", SessionKind::Easy, 30.0, Status::Done, Some(11), 1),
        ];
        let scores = HashMap::new();
        let w = weeks(&sessions, &scores, &HashSet::new(), today);
        // 10/19 yes, 10/26 no, 11/02 yes; the current week has nothing yet
        assert_eq!(
            w.iter().map(|w| w.consistent).collect::<Vec<_>>(),
            vec![Some(true), Some(false), Some(true)]
        );
        let sick: HashSet<NaiveDate> = [d("2026-10-26")].into_iter().collect();
        let w = weeks(&sessions, &scores, &sick, today);
        assert_eq!(w[1].consistent, None);
    }

    #[test]
    fn missions_and_achievements_from_the_plan() {
        let today = d("2026-10-21"); // Wednesday
        let sessions = vec![
            sess(1, "2026-10-13", SessionKind::Easy, 30.0, Status::Done, Some(10), 1),
            sess(2, "2026-10-15", SessionKind::Easy, 30.0, Status::Done, Some(11), 1),
            sess(3, "2026-10-18", SessionKind::Long, 40.0, Status::Done, Some(12), 1),
            sess(4, "2026-10-20", SessionKind::Easy, 30.0, Status::Done, Some(13), 2),
            sess(5, "2026-10-22", SessionKind::Check, 25.0, Status::Planned, None, 2),
            sess(6, "2026-10-25", SessionKind::Long, 45.0, Status::Planned, None, 2),
        ];
        let runs = vec![
            run(10, "2026-10-13", 30.0),
            run(11, "2026-10-15", 30.0),
            run(12, "2026-10-18", 41.0),
            run(13, "2026-10-20", 30.0),
            run(99, "2026-10-16", 20.0),
        ];
        let metrics: HashMap<i64, Metrics> = [10, 11, 12, 13]
            .into_iter()
            .map(|id| (id, easy_metrics(0.02)))
            .collect();
        let st = compute(&Inputs {
            sessions: &sessions,
            runs: &runs,
            metrics: &metrics,
            hr_history: &[],
            goal_reached: None,
            has_goal: true,
            sick_weeks: &HashSet::new(),
            today,
        });
        // the week of 10/12 (3 sessions) and the current one (1 so far) count
        assert_eq!(st.streak, 2);
        assert_eq!(st.weeks.len(), 2);
        assert!(st.weeks[0].perfect && st.weeks[0].two && !st.weeks[1].two);
        let unlocked: Vec<&str> = st
            .achievements
            .iter()
            .filter(|a| a.unlocked.is_some())
            .map(|a| a.id)
            .collect();
        for id in ["first_run", "first_plan", "three_stars", "first_week", "perfect_week"] {
            assert!(unlocked.contains(&id), "{id}: {unlocked:?}");
        }
        assert!(!unlocked.contains(&"streak_4"));
    }
}
