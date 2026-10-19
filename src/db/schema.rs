//! Database schema, as a list of migrations applied in order.
//! `PRAGMA user_version` stores how many have been applied.
//! Never edit a migration that was already released: add a new one.

pub const MIGRATIONS: &[&str] = &[
    // v1 -------------------------------------------------------------------
    r#"
CREATE TABLE settings(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Original files, byte for byte. Never modified.
CREATE TABLE fit_file(
    id          INTEGER PRIMARY KEY,
    sha256      TEXT NOT NULL UNIQUE,
    file_name   TEXT NOT NULL,
    size        INTEGER NOT NULL,
    imported_at INTEGER NOT NULL,
    content     BLOB NOT NULL
);

CREATE TABLE activity(
    id                  INTEGER PRIMARY KEY,
    fit_file_id         INTEGER REFERENCES fit_file(id) ON DELETE SET NULL,
    source              TEXT NOT NULL,          -- 'fit' | 'manual'
    start_time          INTEGER NOT NULL,       -- Unix seconds, UTC
    tz_offset           INTEGER NOT NULL DEFAULT 0,
    modality            TEXT NOT NULL,
    fit_sport           INTEGER,
    fit_sub_sport       INTEGER,
    name                TEXT,
    elapsed_s           REAL,
    timer_s             REAL,
    moving_s            REAL,
    distance_m          REAL,
    distance_override_m REAL,
    calories            REAL,
    avg_hr              REAL,
    max_hr              REAL,
    min_hr              REAL,
    avg_cadence         REAL,
    max_cadence         REAL,
    avg_speed           REAL,
    max_speed           REAL,
    avg_power           REAL,
    max_power           REAL,
    ascent_m            REAL,
    descent_m           REAL,
    min_alt             REAL,
    max_alt             REAL,
    avg_temp            REAL,
    max_temp            REAL,
    training_effect     REAL,
    anaerobic_te        REAL,
    device              TEXT,
    device_serial       INTEGER,
    fit_time_created    INTEGER,
    has_gps             INTEGER NOT NULL DEFAULT 0,
    incline_pct         REAL,
    notes               TEXT,
    rpe                 INTEGER,
    sleep_quality       INTEGER,
    fatigue             INTEGER,
    soreness            INTEGER,
    body_weight         REAL,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);
CREATE INDEX activity_start ON activity(start_time);
CREATE INDEX activity_fit_file ON activity(fit_file_id);

CREATE TABLE lap(
    activity_id INTEGER NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    start_time  INTEGER NOT NULL,
    elapsed_s   REAL,
    timer_s     REAL,
    distance_m  REAL,
    calories    REAL,
    avg_speed   REAL,
    max_speed   REAL,
    avg_hr      REAL,
    max_hr      REAL,
    avg_cadence REAL,
    ascent_m    REAL,
    descent_m   REAL,
    trigger     INTEGER,
    PRIMARY KEY(activity_id, idx)
) WITHOUT ROWID;

CREATE TABLE timer_event(
    activity_id INTEGER NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    t           INTEGER NOT NULL,
    event       INTEGER NOT NULL,
    event_type  INTEGER NOT NULL
);
CREATE INDEX timer_event_activity ON timer_event(activity_id);

CREATE TABLE sample(
    activity_id INTEGER NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    t           INTEGER NOT NULL,
    lat         REAL,
    lon         REAL,
    alt         REAL,
    hr          REAL,
    cadence     REAL,
    distance    REAL,
    speed       REAL,
    power       REAL,
    temp        REAL,
    PRIMARY KEY(activity_id, t)
) WITHOUT ROWID;

-- Sets recorded by the watch itself (FIT 'set' message).
CREATE TABLE fit_set(
    activity_id INTEGER NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    t           INTEGER,
    duration_s  REAL,
    reps        INTEGER,
    weight_kg   REAL,
    category    INTEGER,
    PRIMARY KEY(activity_id, idx)
) WITHOUT ROWID;

-- Derived data: deleted and rebuilt by "Recalculate metrics".
CREATE TABLE activity_metrics(
    activity_id INTEGER PRIMARY KEY REFERENCES activity(id) ON DELETE CASCADE,
    version     INTEGER NOT NULL,
    load        REAL,
    drift_pct   REAL,
    efficiency  REAL,
    flagged     INTEGER NOT NULL DEFAULT 0,
    json        TEXT NOT NULL
);

CREATE TABLE best_effort(
    activity_id INTEGER NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    distance_m  REAL NOT NULL,
    time_s      REAL NOT NULL,
    start_s     REAL NOT NULL,
    PRIMARY KEY(activity_id, distance_m)
) WITHOUT ROWID;

-- Strength training
CREATE TABLE exercise(
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE COLLATE NOCASE,
    muscle_group TEXT,
    archived     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE strength_set(
    id          INTEGER PRIMARY KEY,
    activity_id INTEGER NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    exercise_id INTEGER NOT NULL REFERENCES exercise(id),
    position    INTEGER NOT NULL,
    sets        INTEGER NOT NULL DEFAULT 1,  -- "3 x 10 x 60 kg" is one row
    reps        INTEGER,
    weight_kg   REAL,
    rpe         REAL,
    rest_s      REAL,
    notes       TEXT
);
CREATE INDEX strength_set_activity ON strength_set(activity_id);
CREATE INDEX strength_set_exercise ON strength_set(exercise_id);

-- Plans and calendar (dates are local 'YYYY-MM-DD')
CREATE TABLE plan(
    id                INTEGER PRIMARY KEY,
    name              TEXT NOT NULL,
    goal              TEXT,
    objectives        TEXT,
    start_date        TEXT,
    end_date          TEXT,
    sessions_per_week INTEGER,
    modalities        TEXT,
    created_at        INTEGER NOT NULL
);

CREATE TABLE reference_test(
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    modality    TEXT NOT NULL,
    distance_m  REAL NOT NULL,
    target_pace REAL,
    incline_pct REAL,
    notes       TEXT
);

CREATE TABLE reference_test_run(
    test_id     INTEGER NOT NULL REFERENCES reference_test(id) ON DELETE CASCADE,
    activity_id INTEGER NOT NULL REFERENCES activity(id) ON DELETE CASCADE,
    PRIMARY KEY(test_id, activity_id)
) WITHOUT ROWID;

CREATE TABLE planned_workout(
    id                INTEGER PRIMARY KEY,
    plan_id           INTEGER REFERENCES plan(id) ON DELETE SET NULL,
    date              TEXT NOT NULL,
    modality          TEXT NOT NULL,
    run_type          TEXT,
    title             TEXT,
    duration_s        REAL,
    distance_m        REAL,
    target_pace       REAL,
    target_hr         REAL,
    target_zone       INTEGER,
    incline_pct       REAL,
    notes             TEXT,
    steps             TEXT,          -- JSON: structured interval blocks
    activity_id       INTEGER REFERENCES activity(id) ON DELETE SET NULL,
    reference_test_id INTEGER REFERENCES reference_test(id) ON DELETE SET NULL
);
CREATE INDEX planned_workout_date ON planned_workout(date);

-- Goals and long-term objectives
CREATE TABLE objective(
    id          INTEGER PRIMARY KEY,
    title       TEXT NOT NULL,
    description TEXT,
    target_date TEXT,
    created_at  INTEGER NOT NULL
);

CREATE TABLE goal(
    id           INTEGER PRIMARY KEY,
    kind         TEXT NOT NULL,
    title        TEXT NOT NULL,
    params       TEXT NOT NULL,      -- JSON, depends on kind
    start_date   TEXT,
    end_date     TEXT,
    objective_id INTEGER REFERENCES objective(id) ON DELETE CASCADE,
    position     INTEGER NOT NULL DEFAULT 0,
    archived     INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL
);
"#,
    // v2 -------------------------------------------------------------------
    // FitLedger 0.2 becomes a running coach: manual plans, generic goals,
    // tennis and strength tables go away (a backup is saved before migrating).
    r#"
DROP TABLE IF EXISTS strength_set;
DROP TABLE IF EXISTS exercise;
DROP TABLE IF EXISTS planned_workout;
DROP TABLE IF EXISTS plan;
DROP TABLE IF EXISTS reference_test_run;
DROP TABLE IF EXISTS reference_test;
DROP TABLE IF EXISTS goal;
DROP TABLE IF EXISTS objective;

-- One row per goal version; only one is active.
CREATE TABLE coach_goal(
    id          INTEGER PRIMARY KEY,
    created_at  INTEGER NOT NULL,
    active      INTEGER NOT NULL DEFAULT 1,
    json        TEXT NOT NULL,
    achieved_on TEXT,
    achieved_by INTEGER REFERENCES activity(id) ON DELETE SET NULL
);

-- Generated sessions. Future ones are rebuilt; past ones keep their result.
CREATE TABLE coach_session(
    id          INTEGER PRIMARY KEY,
    goal_id     INTEGER NOT NULL REFERENCES coach_goal(id) ON DELETE CASCADE,
    date        TEXT NOT NULL,
    json        TEXT NOT NULL,
    status      TEXT NOT NULL,
    activity_id INTEGER REFERENCES activity(id) ON DELETE SET NULL
);
CREATE INDEX coach_session_goal_date ON coach_session(goal_id, date);

-- What the coach noticed and changed, with the data behind it.
CREATE TABLE coach_log(
    id       INTEGER PRIMARY KEY,
    at       INTEGER NOT NULL,
    goal_id  INTEGER REFERENCES coach_goal(id) ON DELETE CASCADE,
    notes    TEXT NOT NULL,
    evidence TEXT NOT NULL
);
"#,
];

pub fn latest_version() -> i64 {
    MIGRATIONS.len() as i64
}
