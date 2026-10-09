//! Plans, planned workouts (with structured intervals), reference tests,
//! goals, long-term objectives and strength training: types and storage.

use crate::db::{Db, Result, now};
use crate::model::Modality;
use chrono::NaiveDate;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub fn date_str(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

// ------------------------------------------------------------------ plans --

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub id: i64,
    pub name: String,
    pub goal: String,
    /// One objective per line.
    pub objectives: String,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub sessions_per_week: Option<i64>,
    pub modalities: Vec<Modality>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunType {
    #[default]
    Easy,
    Recovery,
    Long,
    Intervals,
    Progressive,
    Tempo,
    Fartlek,
    Test,
    Free,
}

impl RunType {
    pub const ALL: [RunType; 9] = [
        RunType::Easy,
        RunType::Recovery,
        RunType::Long,
        RunType::Intervals,
        RunType::Progressive,
        RunType::Tempo,
        RunType::Fartlek,
        RunType::Test,
        RunType::Free,
    ];

    pub fn code(self) -> &'static str {
        match self {
            RunType::Easy => "easy",
            RunType::Recovery => "recovery",
            RunType::Long => "long",
            RunType::Intervals => "intervals",
            RunType::Progressive => "progressive",
            RunType::Tempo => "tempo",
            RunType::Fartlek => "fartlek",
            RunType::Test => "test",
            RunType::Free => "free",
        }
    }

    pub fn from_code(s: &str) -> Option<RunType> {
        RunType::ALL.into_iter().find(|r| r.code() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            RunType::Easy => tr!("Corrida fácil", "Easy run"),
            RunType::Recovery => tr!("Recuperação", "Recovery"),
            RunType::Long => tr!("Longão", "Long run"),
            RunType::Intervals => tr!("Intervalado", "Intervals"),
            RunType::Progressive => tr!("Progressivo", "Progressive"),
            RunType::Tempo => tr!("Tempo run", "Tempo run"),
            RunType::Fartlek => tr!("Fartlek", "Fartlek"),
            RunType::Test => tr!("Teste", "Test"),
            RunType::Free => tr!("Corrida livre", "Free run"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Warmup,
    #[default]
    Work,
    Recovery,
    Cooldown,
    Steady,
}

impl StepKind {
    pub const ALL: [StepKind; 5] = [
        StepKind::Warmup,
        StepKind::Work,
        StepKind::Recovery,
        StepKind::Cooldown,
        StepKind::Steady,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StepKind::Warmup => tr!("Aquecimento", "Warm-up"),
            StepKind::Work => tr!("Forte", "Hard"),
            StepKind::Recovery => tr!("Leve", "Easy"),
            StepKind::Cooldown => tr!("Desaquecimento", "Cool-down"),
            StepKind::Steady => tr!("Contínuo", "Steady"),
        }
    }
}

/// One block of a structured workout. `Repeat` holds the blocks repeated N times.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Step {
    Block {
        kind: StepKind,
        duration_s: Option<f64>,
        distance_m: Option<f64>,
        /// Target pace, s/km.
        pace: Option<f64>,
        hr_zone: Option<u8>,
        #[serde(default)]
        note: String,
    },
    Repeat {
        times: u32,
        steps: Vec<Step>,
    },
}

impl Step {
    pub fn block(kind: StepKind) -> Step {
        Step::Block {
            kind,
            duration_s: None,
            distance_m: None,
            pace: None,
            hr_zone: None,
            note: String::new(),
        }
    }

    /// (seconds, meters) of the whole step, when they can be computed from the targets.
    pub fn totals(&self) -> (f64, f64) {
        match self {
            Step::Block {
                duration_s,
                distance_m,
                pace,
                ..
            } => {
                let t = duration_s
                    .or_else(|| Some(distance_m.as_ref()? / 1000.0 * pace.as_ref()?))
                    .unwrap_or(0.0);
                let d = distance_m
                    .or_else(|| Some(duration_s.as_ref()? / pace.as_ref()? * 1000.0))
                    .unwrap_or(0.0);
                (t, d)
            }
            Step::Repeat { times, steps } => {
                let (t, d) = steps
                    .iter()
                    .map(Step::totals)
                    .fold((0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1));
                (t * *times as f64, d * *times as f64)
            }
        }
    }
}

pub fn steps_totals(steps: &[Step]) -> (f64, f64) {
    steps
        .iter()
        .map(Step::totals)
        .fold((0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1))
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedWorkout {
    pub id: i64,
    pub plan_id: Option<i64>,
    pub date: NaiveDate,
    pub modality: Modality,
    pub run_type: Option<RunType>,
    pub title: String,
    pub duration_s: Option<f64>,
    pub distance_m: Option<f64>,
    pub target_pace: Option<f64>,
    pub target_hr: Option<f64>,
    pub target_zone: Option<i64>,
    pub incline_pct: Option<f64>,
    pub notes: String,
    pub steps: Vec<Step>,
    pub activity_id: Option<i64>,
    pub reference_test_id: Option<i64>,
}

impl PlannedWorkout {
    pub fn new(date: NaiveDate) -> Self {
        PlannedWorkout {
            id: 0,
            plan_id: None,
            date,
            modality: Modality::RoadRun,
            run_type: Some(RunType::Easy),
            title: String::new(),
            duration_s: None,
            distance_m: None,
            target_pace: None,
            target_hr: None,
            target_zone: None,
            incline_pct: None,
            notes: String::new(),
            steps: Vec::new(),
            activity_id: None,
            reference_test_id: None,
        }
    }

    pub fn display_title(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.clone();
        }
        match (self.modality.is_run(), self.run_type) {
            (true, Some(r)) => r.label().to_string(),
            _ => self.modality.label().to_string(),
        }
    }
}

// --------------------------------------------------------- reference tests --

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RefTest {
    pub id: i64,
    pub name: String,
    pub modality: Modality,
    pub distance_m: f64,
    pub target_pace: Option<f64>,
    pub incline_pct: Option<f64>,
    pub notes: String,
}

// ------------------------------------------------------------------- goals --

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    /// Distance in a period ("100 km in the month").
    #[default]
    Distance,
    /// Sessions of a modality per week ("run 3 times a week").
    Frequency,
    /// Distance under a time ("5 km under 28 minutes").
    Performance,
    /// Distance at a pace with average HR below a value.
    HeartRate,
    /// Longest distance in one run ("reach 10 km").
    MaxDistance,
    /// Hours of a modality in a period ("tennis 4 hours a month").
    Hours,
}

impl GoalKind {
    pub const ALL: [GoalKind; 6] = [
        GoalKind::Distance,
        GoalKind::Frequency,
        GoalKind::Performance,
        GoalKind::HeartRate,
        GoalKind::MaxDistance,
        GoalKind::Hours,
    ];

    pub fn code(self) -> &'static str {
        match self {
            GoalKind::Distance => "distance",
            GoalKind::Frequency => "frequency",
            GoalKind::Performance => "performance",
            GoalKind::HeartRate => "heart_rate",
            GoalKind::MaxDistance => "max_distance",
            GoalKind::Hours => "hours",
        }
    }

    pub fn from_code(s: &str) -> GoalKind {
        GoalKind::ALL.into_iter().find(|k| k.code() == s).unwrap_or_default()
    }

    pub fn label(self) -> &'static str {
        match self {
            GoalKind::Distance => tr!("Distância no período", "Distance in a period"),
            GoalKind::Frequency => tr!("Frequência semanal", "Weekly frequency"),
            GoalKind::Performance => tr!("Performance (tempo)", "Performance (time)"),
            GoalKind::HeartRate => tr!("Frequência cardíaca no pace", "Heart rate at pace"),
            GoalKind::MaxDistance => tr!("Distância máxima", "Longest distance"),
            GoalKind::Hours => tr!("Horas no período", "Hours in a period"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalPeriod {
    Week,
    #[default]
    Month,
    /// Between the goal's start and end dates (or ever).
    Range,
}

impl GoalPeriod {
    pub fn label(self) -> &'static str {
        match self {
            GoalPeriod::Week => tr!("por semana", "per week"),
            GoalPeriod::Month => tr!("por mês", "per month"),
            GoalPeriod::Range => tr!("no período da meta", "in the goal period"),
        }
    }
}

/// Parameters of a goal; which fields matter depends on the kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GoalParams {
    /// Modality code; None = all runs (road + treadmill).
    pub modality: Option<String>,
    pub period: GoalPeriod,
    pub distance_m: Option<f64>,
    pub time_s: Option<f64>,
    pub pace: Option<f64>,
    pub hr: Option<f64>,
    pub count: Option<f64>,
    pub hours: Option<f64>,
}

impl GoalParams {
    /// Activities that count for this goal.
    pub fn matches(&self, m: Modality) -> bool {
        match &self.modality {
            Some(code) => Modality::from_code(code) == m,
            None => m.is_run(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Goal {
    pub id: i64,
    pub kind: GoalKind,
    pub title: String,
    pub params: GoalParams,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub objective_id: Option<i64>,
    pub position: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Objective {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub target_date: Option<NaiveDate>,
}

// ---------------------------------------------------------------- strength --

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Exercise {
    pub id: i64,
    pub name: String,
    pub muscle_group: String,
    pub archived: bool,
}

/// "3 × 10 × 60 kg" of one exercise in one session.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SetGroup {
    pub id: i64,
    pub activity_id: i64,
    pub exercise_id: i64,
    pub position: i64,
    pub sets: i64,
    pub reps: Option<i64>,
    pub weight_kg: Option<f64>,
    pub rpe: Option<f64>,
    pub rest_s: Option<f64>,
    pub notes: String,
}

impl SetGroup {
    /// séries × repetições × peso
    pub fn volume(&self) -> f64 {
        self.sets as f64 * self.reps.unwrap_or(0) as f64 * self.weight_kg.unwrap_or(0.0)
    }
}

/// Exercises created on first use, in the current language: (name, muscle group).
pub fn default_exercises() -> [(&'static str, &'static str); 7] {
    [
        (tr!("Supino", "Bench press"), tr!("Peito", "Chest")),
        (tr!("Agachamento", "Squat"), tr!("Pernas", "Legs")),
        (tr!("Leg press", "Leg press"), tr!("Pernas", "Legs")),
        (tr!("Remada", "Row"), tr!("Costas", "Back")),
        (tr!("Desenvolvimento", "Shoulder press"), tr!("Ombros", "Shoulders")),
        (tr!("Rosca", "Biceps curl"), tr!("Bíceps", "Biceps")),
        (tr!("Tríceps", "Triceps"), tr!("Tríceps", "Triceps")),
    ]
}

// ------------------------------------------------------------- persistence --

fn opt_date(s: Option<String>) -> Option<NaiveDate> {
    s.as_deref().and_then(parse_date)
}

impl Db {
    // plans
    pub fn plans(&self) -> Result<Vec<Plan>> {
        let mut st = self.conn.prepare(
            "SELECT id, name, goal, objectives, start_date, end_date, sessions_per_week, modalities
             FROM plan ORDER BY coalesce(start_date, '9999') DESC, id DESC",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Plan {
                id: r.get(0)?,
                name: r.get(1)?,
                goal: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                objectives: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                start_date: opt_date(r.get(4)?),
                end_date: opt_date(r.get(5)?),
                sessions_per_week: r.get(6)?,
                modalities: r
                    .get::<_, Option<String>>(7)?
                    .unwrap_or_default()
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(Modality::from_code)
                    .collect(),
            })
        })?;
        rows.collect()
    }

    pub fn save_plan(&self, p: &Plan) -> Result<i64> {
        let mods = p.modalities.iter().map(|m| m.code()).collect::<Vec<_>>().join(",");
        let args = params![
            p.name,
            p.goal,
            p.objectives,
            p.start_date.map(date_str),
            p.end_date.map(date_str),
            p.sessions_per_week,
            mods
        ];
        if p.id == 0 {
            self.conn.execute(
                "INSERT INTO plan(name, goal, objectives, start_date, end_date, sessions_per_week, modalities, created_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    p.name,
                    p.goal,
                    p.objectives,
                    p.start_date.map(date_str),
                    p.end_date.map(date_str),
                    p.sessions_per_week,
                    mods,
                    now()
                ],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE plan SET name=?1, goal=?2, objectives=?3, start_date=?4, end_date=?5,
                    sessions_per_week=?6, modalities=?7 WHERE id=?8",
                rusqlite::params_from_iter(
                    args.iter()
                        .copied()
                        .chain(std::iter::once(&p.id as &dyn rusqlite::ToSql)),
                ),
            )?;
            Ok(p.id)
        }
    }

    /// Deletes the plan; its planned workouts stay in the calendar without a plan.
    pub fn delete_plan(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM plan WHERE id = ?1", [id])?;
        Ok(())
    }

    // planned workouts
    pub fn planned_workouts(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<PlannedWorkout>> {
        let mut st = self
            .conn
            .prepare(&format!("{PW_SELECT} WHERE date BETWEEN ?1 AND ?2 ORDER BY date, id"))?;
        let rows = st.query_map([date_str(from), date_str(to)], pw_from_row)?;
        rows.collect()
    }

    pub fn planned_workouts_of_plan(&self, plan_id: i64) -> Result<Vec<PlannedWorkout>> {
        let mut st = self
            .conn
            .prepare(&format!("{PW_SELECT} WHERE plan_id = ?1 ORDER BY date, id"))?;
        let rows = st.query_map([plan_id], pw_from_row)?;
        rows.collect()
    }

    pub fn planned_workout(&self, id: i64) -> Result<Option<PlannedWorkout>> {
        self.conn
            .query_row(&format!("{PW_SELECT} WHERE id = ?1"), [id], pw_from_row)
            .optional()
    }

    pub fn save_planned_workout(&self, w: &PlannedWorkout) -> Result<i64> {
        let steps = serde_json::to_string(&w.steps).unwrap_or_else(|_| "[]".into());
        let vals = params![
            w.plan_id,
            date_str(w.date),
            w.modality.code(),
            w.run_type.map(|r| r.code()),
            w.title,
            w.duration_s,
            w.distance_m,
            w.target_pace,
            w.target_hr,
            w.target_zone,
            w.incline_pct,
            w.notes,
            steps,
            w.activity_id,
            w.reference_test_id
        ];
        if w.id == 0 {
            self.conn.execute(
                "INSERT INTO planned_workout(plan_id, date, modality, run_type, title, duration_s, distance_m,
                    target_pace, target_hr, target_zone, incline_pct, notes, steps, activity_id, reference_test_id)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                vals,
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE planned_workout SET plan_id=?1, date=?2, modality=?3, run_type=?4, title=?5, duration_s=?6,
                    distance_m=?7, target_pace=?8, target_hr=?9, target_zone=?10, incline_pct=?11, notes=?12,
                    steps=?13, activity_id=?14, reference_test_id=?15 WHERE id=?16",
                rusqlite::params_from_iter(
                    vals.iter()
                        .copied()
                        .chain(std::iter::once(&w.id as &dyn rusqlite::ToSql)),
                ),
            )?;
            Ok(w.id)
        }
    }

    pub fn delete_planned_workout(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM planned_workout WHERE id = ?1", [id])?;
        Ok(())
    }

    // reference tests
    pub fn reference_tests(&self) -> Result<Vec<RefTest>> {
        let mut st = self.conn.prepare(
            "SELECT id, name, modality, distance_m, target_pace, incline_pct, notes FROM reference_test ORDER BY id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(RefTest {
                id: r.get(0)?,
                name: r.get(1)?,
                modality: Modality::from_code(&r.get::<_, String>(2)?),
                distance_m: r.get(3)?,
                target_pace: r.get(4)?,
                incline_pct: r.get(5)?,
                notes: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
            })
        })?;
        rows.collect()
    }

    pub fn save_reference_test(&self, t: &RefTest) -> Result<i64> {
        if t.id == 0 {
            self.conn.execute(
                "INSERT INTO reference_test(name, modality, distance_m, target_pace, incline_pct, notes)
                 VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    t.name,
                    t.modality.code(),
                    t.distance_m,
                    t.target_pace,
                    t.incline_pct,
                    t.notes
                ],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE reference_test SET name=?1, modality=?2, distance_m=?3, target_pace=?4, incline_pct=?5, notes=?6
                 WHERE id=?7",
                params![t.name, t.modality.code(), t.distance_m, t.target_pace, t.incline_pct, t.notes, t.id],
            )?;
            Ok(t.id)
        }
    }

    pub fn delete_reference_test(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM reference_test WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn reference_runs(&self, test_id: i64) -> Result<Vec<i64>> {
        let mut st = self.conn.prepare(
            "SELECT r.activity_id FROM reference_test_run r JOIN activity a ON a.id = r.activity_id
             WHERE r.test_id = ?1 ORDER BY a.start_time",
        )?;
        let rows = st.query_map([test_id], |r| r.get(0))?;
        rows.collect()
    }

    /// (test id, name, is this activity a run of it).
    pub fn reference_tests_for_activity(&self, activity_id: i64) -> Result<Vec<(i64, String, bool)>> {
        let mut st = self.conn.prepare(
            "SELECT t.id, t.name, EXISTS(SELECT 1 FROM reference_test_run r WHERE r.test_id = t.id AND r.activity_id = ?1)
             FROM reference_test t ORDER BY t.id",
        )?;
        let rows = st.query_map([activity_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect()
    }

    pub fn set_reference_run(&self, test_id: i64, activity_id: i64, on: bool) -> Result<()> {
        if on {
            self.conn.execute(
                "INSERT OR IGNORE INTO reference_test_run(test_id, activity_id) VALUES(?1, ?2)",
                params![test_id, activity_id],
            )?;
        } else {
            self.conn.execute(
                "DELETE FROM reference_test_run WHERE test_id = ?1 AND activity_id = ?2",
                params![test_id, activity_id],
            )?;
        }
        Ok(())
    }

    // goals and objectives
    pub fn goals(&self) -> Result<Vec<Goal>> {
        let mut st = self.conn.prepare(
            "SELECT id, kind, title, params, start_date, end_date, objective_id, position, archived
             FROM goal ORDER BY archived, objective_id IS NOT NULL, position, id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Goal {
                id: r.get(0)?,
                kind: GoalKind::from_code(&r.get::<_, String>(1)?),
                title: r.get(2)?,
                params: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default(),
                start_date: opt_date(r.get(4)?),
                end_date: opt_date(r.get(5)?),
                objective_id: r.get(6)?,
                position: r.get(7)?,
                archived: r.get(8)?,
            })
        })?;
        rows.collect()
    }

    pub fn save_goal(&self, g: &Goal) -> Result<i64> {
        let p = serde_json::to_string(&g.params).unwrap_or_else(|_| "{}".into());
        if g.id == 0 {
            self.conn.execute(
                "INSERT INTO goal(kind, title, params, start_date, end_date, objective_id, position, archived, created_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    g.kind.code(),
                    g.title,
                    p,
                    g.start_date.map(date_str),
                    g.end_date.map(date_str),
                    g.objective_id,
                    g.position,
                    g.archived,
                    now()
                ],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE goal SET kind=?1, title=?2, params=?3, start_date=?4, end_date=?5, objective_id=?6,
                    position=?7, archived=?8 WHERE id=?9",
                params![
                    g.kind.code(),
                    g.title,
                    p,
                    g.start_date.map(date_str),
                    g.end_date.map(date_str),
                    g.objective_id,
                    g.position,
                    g.archived,
                    g.id
                ],
            )?;
            Ok(g.id)
        }
    }

    pub fn delete_goal(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM goal WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn objectives(&self) -> Result<Vec<Objective>> {
        let mut st = self
            .conn
            .prepare("SELECT id, title, description, target_date FROM objective ORDER BY id")?;
        let rows = st.query_map([], |r| {
            Ok(Objective {
                id: r.get(0)?,
                title: r.get(1)?,
                description: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                target_date: opt_date(r.get(3)?),
            })
        })?;
        rows.collect()
    }

    pub fn save_objective(&self, o: &Objective) -> Result<i64> {
        if o.id == 0 {
            self.conn.execute(
                "INSERT INTO objective(title, description, target_date, created_at) VALUES(?1,?2,?3,?4)",
                params![o.title, o.description, o.target_date.map(date_str), now()],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE objective SET title=?1, description=?2, target_date=?3 WHERE id=?4",
                params![o.title, o.description, o.target_date.map(date_str), o.id],
            )?;
            Ok(o.id)
        }
    }

    /// Deletes the objective and its sub-goals.
    pub fn delete_objective(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM objective WHERE id = ?1", [id])?;
        Ok(())
    }

    // strength
    pub fn exercises(&self) -> Result<Vec<Exercise>> {
        let mut st = self
            .conn
            .prepare("SELECT id, name, muscle_group, archived FROM exercise ORDER BY archived, name")?;
        let rows = st.query_map([], |r| {
            Ok(Exercise {
                id: r.get(0)?,
                name: r.get(1)?,
                muscle_group: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                archived: r.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// Creates the default exercises the first time.
    pub fn ensure_default_exercises(&self) -> Result<()> {
        let n: i64 = self.conn.query_row("SELECT count(*) FROM exercise", [], |r| r.get(0))?;
        if n == 0 && self.get_kv("exercises_seeded").is_none() {
            for (name, group) in default_exercises() {
                self.conn.execute(
                    "INSERT OR IGNORE INTO exercise(name, muscle_group) VALUES(?1, ?2)",
                    params![name, group],
                )?;
            }
            self.set_kv("exercises_seeded", "1")?;
        }
        Ok(())
    }

    pub fn save_exercise(&self, e: &Exercise) -> Result<i64> {
        if e.id == 0 {
            self.conn.execute(
                "INSERT INTO exercise(name, muscle_group, archived) VALUES(?1,?2,?3)",
                params![e.name.trim(), e.muscle_group, e.archived],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE exercise SET name=?1, muscle_group=?2, archived=?3 WHERE id=?4",
                params![e.name.trim(), e.muscle_group, e.archived, e.id],
            )?;
            Ok(e.id)
        }
    }

    pub fn set_groups(&self, activity_id: i64) -> Result<Vec<SetGroup>> {
        let mut st = self
            .conn
            .prepare(&format!("{SG_SELECT} WHERE activity_id = ?1 ORDER BY position, id"))?;
        let rows = st.query_map([activity_id], sg_from_row)?;
        rows.collect()
    }

    /// Every set group of an exercise with the activity start time, oldest first.
    pub fn exercise_history(&self, exercise_id: i64) -> Result<Vec<(i64, SetGroup)>> {
        let mut st = self.conn.prepare(
            "SELECT a.start_time + a.tz_offset, s.id, s.activity_id, s.exercise_id, s.position, s.sets, s.reps,
                    s.weight_kg, s.rpe, s.rest_s, s.notes
             FROM strength_set s JOIN activity a ON a.id = s.activity_id
             WHERE s.exercise_id = ?1 ORDER BY a.start_time, s.position",
        )?;
        let rows = st.query_map([exercise_id], |r| {
            Ok((
                r.get(0)?,
                SetGroup {
                    id: r.get(1)?,
                    activity_id: r.get(2)?,
                    exercise_id: r.get(3)?,
                    position: r.get(4)?,
                    sets: r.get(5)?,
                    reps: r.get(6)?,
                    weight_kg: r.get(7)?,
                    rpe: r.get(8)?,
                    rest_s: r.get(9)?,
                    notes: r.get::<_, Option<String>>(10)?.unwrap_or_default(),
                },
            ))
        })?;
        rows.collect()
    }

    /// Strength volume (kg) per activity.
    pub fn strength_volumes(&self) -> Result<std::collections::HashMap<i64, f64>> {
        let mut st = self.conn.prepare(
            "SELECT activity_id, sum(sets * coalesce(reps, 0) * coalesce(weight_kg, 0)) FROM strength_set GROUP BY activity_id",
        )?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get::<_, Option<f64>>(1)?.unwrap_or(0.0))))?;
        rows.collect()
    }

    /// Replaces all set groups of an activity.
    pub fn save_set_groups(&mut self, activity_id: i64, groups: &[SetGroup]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM strength_set WHERE activity_id = ?1", [activity_id])?;
        for (i, g) in groups.iter().enumerate() {
            if g.exercise_id == 0 {
                continue;
            }
            tx.execute(
                "INSERT INTO strength_set(activity_id, exercise_id, position, sets, reps, weight_kg, rpe, rest_s, notes)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![activity_id, g.exercise_id, i as i64, g.sets.max(1), g.reps, g.weight_kg, g.rpe, g.rest_s, g.notes],
            )?;
        }
        tx.commit()
    }
}

const PW_SELECT: &str = "SELECT id, plan_id, date, modality, run_type, title, duration_s, distance_m, target_pace,
    target_hr, target_zone, incline_pct, notes, steps, activity_id, reference_test_id FROM planned_workout";

fn pw_from_row(r: &rusqlite::Row) -> Result<PlannedWorkout> {
    Ok(PlannedWorkout {
        id: r.get(0)?,
        plan_id: r.get(1)?,
        date: parse_date(&r.get::<_, String>(2)?).unwrap_or_default(),
        modality: Modality::from_code(&r.get::<_, String>(3)?),
        run_type: r.get::<_, Option<String>>(4)?.as_deref().and_then(RunType::from_code),
        title: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
        duration_s: r.get(6)?,
        distance_m: r.get(7)?,
        target_pace: r.get(8)?,
        target_hr: r.get(9)?,
        target_zone: r.get(10)?,
        incline_pct: r.get(11)?,
        notes: r.get::<_, Option<String>>(12)?.unwrap_or_default(),
        steps: r
            .get::<_, Option<String>>(13)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        activity_id: r.get(14)?,
        reference_test_id: r.get(15)?,
    })
}

const SG_SELECT: &str =
    "SELECT id, activity_id, exercise_id, position, sets, reps, weight_kg, rpe, rest_s, notes FROM strength_set";

fn sg_from_row(r: &rusqlite::Row) -> Result<SetGroup> {
    Ok(SetGroup {
        id: r.get(0)?,
        activity_id: r.get(1)?,
        exercise_id: r.get(2)?,
        position: r.get(3)?,
        sets: r.get(4)?,
        reps: r.get(5)?,
        weight_kg: r.get(6)?,
        rpe: r.get(7)?,
        rest_s: r.get(8)?,
        notes: r.get::<_, Option<String>>(9)?.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).unwrap()
    }

    #[test]
    fn interval_totals() {
        // 10 min @7:00 + 5 × (3 min @5:30 + 2 min @7:30) + 10 min easy
        let steps = vec![
            Step::Block {
                kind: StepKind::Warmup,
                duration_s: Some(600.0),
                distance_m: None,
                pace: Some(420.0),
                hr_zone: None,
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
                        hr_zone: None,
                        note: String::new(),
                    },
                    Step::Block {
                        kind: StepKind::Recovery,
                        duration_s: Some(120.0),
                        distance_m: None,
                        pace: Some(450.0),
                        hr_zone: None,
                        note: String::new(),
                    },
                ],
            },
            Step::Block {
                kind: StepKind::Cooldown,
                duration_s: Some(600.0),
                distance_m: None,
                pace: None,
                hr_zone: None,
                note: String::new(),
            },
        ];
        let (t, dist) = steps_totals(&steps);
        assert_eq!(t, 600.0 + 5.0 * 300.0 + 600.0);
        let expected = 600.0 / 420.0 * 1000.0 + 5.0 * (180.0 / 330.0 + 120.0 / 450.0) * 1000.0;
        assert!((dist - expected).abs() < 1e-6);
        // JSON roundtrip
        let json = serde_json::to_string(&steps).unwrap();
        let back: Vec<Step> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, steps);
    }

    #[test]
    fn plans_and_planned_workouts_persist() {
        let db = Db::open_in_memory().unwrap();
        let pid = db
            .save_plan(&Plan {
                name: "Melhorar eficiência nos 5 km".into(),
                start_date: Some(d("2026-10-01")),
                end_date: Some(d("2026-12-31")),
                sessions_per_week: Some(5),
                modalities: vec![Modality::RoadRun, Modality::Tennis],
                ..Default::default()
            })
            .unwrap();
        let mut p = db.plans().unwrap().remove(0);
        assert_eq!(p.modalities, vec![Modality::RoadRun, Modality::Tennis]);
        p.goal = "5 km a 6:00/km".into();
        db.save_plan(&p).unwrap();
        assert_eq!(db.plans().unwrap()[0].goal, "5 km a 6:00/km");

        let mut w = PlannedWorkout::new(d("2026-10-12"));
        w.plan_id = Some(pid);
        w.run_type = Some(RunType::Intervals);
        w.steps = vec![Step::block(StepKind::Warmup)];
        let wid = db.save_planned_workout(&w).unwrap();
        let got = db.planned_workouts(d("2026-10-01"), d("2026-10-31")).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].run_type, Some(RunType::Intervals));
        assert_eq!(got[0].steps.len(), 1);
        assert!(
            db.planned_workouts(d("2026-11-01"), d("2026-11-30"))
                .unwrap()
                .is_empty()
        );
        db.delete_plan(pid).unwrap();
        assert_eq!(db.planned_workout(wid).unwrap().unwrap().plan_id, None);
    }

    #[test]
    fn goals_strength_and_tests_persist() {
        let mut db = Db::open_in_memory().unwrap();
        let oid = db
            .save_objective(&Objective {
                title: "Correr 10 km".into(),
                ..Default::default()
            })
            .unwrap();
        let g = Goal {
            kind: GoalKind::MaxDistance,
            title: "5 km".into(),
            params: GoalParams {
                distance_m: Some(5000.0),
                ..Default::default()
            },
            objective_id: Some(oid),
            ..Default::default()
        };
        db.save_goal(&g).unwrap();
        assert_eq!(db.goals().unwrap()[0].params.distance_m, Some(5000.0));
        db.delete_objective(oid).unwrap();
        assert!(db.goals().unwrap().is_empty());

        db.ensure_default_exercises().unwrap();
        let ex = db.exercises().unwrap();
        assert_eq!(ex.len(), 7);
        let a = crate::model::Activity {
            modality: Modality::Strength,
            start_time: 1_780_000_000,
            elapsed_s: Some(3600.0),
            ..Default::default()
        };
        let aid = db.save_manual_activity(&a).unwrap();
        let sg = SetGroup {
            exercise_id: ex[0].id,
            sets: 3,
            reps: Some(10),
            weight_kg: Some(60.0),
            ..Default::default()
        };
        assert_eq!(sg.volume(), 1800.0);
        db.save_set_groups(aid, &[sg]).unwrap();
        assert_eq!(db.exercise_history(ex[0].id).unwrap().len(), 1);
        assert_eq!(db.strength_volumes().unwrap()[&aid], 1800.0);

        let tid = db
            .save_reference_test(&RefTest {
                name: "5 km 6:00".into(),
                modality: Modality::Treadmill,
                distance_m: 5000.0,
                ..Default::default()
            })
            .unwrap();
        db.set_reference_run(tid, aid, true).unwrap();
        assert_eq!(db.reference_runs(tid).unwrap(), vec![aid]);
        assert!(db.reference_tests_for_activity(aid).unwrap()[0].2);
        db.delete_activity(aid).unwrap();
        assert!(db.reference_runs(tid).unwrap().is_empty());
    }
}
