//! Domain types shared by import, database, analysis and UI.
//! Units are always SI in storage: meters, seconds, m/s, °C, Unix seconds (UTC).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Modality {
    RoadRun,
    Treadmill,
    Tennis,
    Strength,
    #[default]
    Other,
}

impl Modality {
    pub const ALL: [Modality; 5] = [
        Modality::RoadRun,
        Modality::Treadmill,
        Modality::Tennis,
        Modality::Strength,
        Modality::Other,
    ];

    /// Code stored in the database (never translated).
    pub fn code(self) -> &'static str {
        match self {
            Modality::RoadRun => "road_run",
            Modality::Treadmill => "treadmill",
            Modality::Tennis => "tennis",
            Modality::Strength => "strength",
            Modality::Other => "other",
        }
    }

    pub fn from_code(c: &str) -> Modality {
        Modality::ALL.into_iter().find(|m| m.code() == c).unwrap_or(Modality::Other)
    }

    pub fn label(self) -> &'static str {
        match self {
            Modality::RoadRun => tr!("Corrida de rua", "Road run"),
            Modality::Treadmill => tr!("Esteira", "Treadmill"),
            Modality::Tennis => tr!("Tênis", "Tennis"),
            Modality::Strength => tr!("Musculação", "Strength"),
            Modality::Other => tr!("Outras", "Other"),
        }
    }

    pub fn is_run(self) -> bool {
        matches!(self, Modality::RoadRun | Modality::Treadmill)
    }
}

/// One point of the activity stream (usually one per second).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub t: i64,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub alt: Option<f64>,
    pub hr: Option<f64>,
    /// Steps per minute for running; rpm otherwise.
    pub cadence: Option<f64>,
    /// Accumulated distance in meters.
    pub distance: Option<f64>,
    pub speed: Option<f64>,
    pub power: Option<f64>,
    pub temp: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lap {
    pub start: i64,
    pub elapsed_s: Option<f64>,
    pub timer_s: Option<f64>,
    pub distance_m: Option<f64>,
    pub calories: Option<f64>,
    pub avg_speed: Option<f64>,
    pub max_speed: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub trigger: Option<i64>,
}

/// FIT timer event (start/stop), used to find pauses.
#[derive(Debug, Clone, PartialEq)]
pub struct TimerEvent {
    pub t: i64,
    pub event: i64,
    pub event_type: i64,
}

/// A strength set recorded by the watch (FIT `set` message).
#[derive(Debug, Clone, PartialEq)]
pub struct StrengthSet {
    pub t: Option<i64>,
    pub duration_s: Option<f64>,
    pub reps: Option<i64>,
    pub weight_kg: Option<f64>,
    pub category: Option<i64>,
}

/// One row of the `activity` table, with user edits applied.
#[derive(Debug, Clone, Default)]
pub struct Activity {
    pub id: i64,
    pub fit_file_id: Option<i64>,
    pub source: String,
    pub start_time: i64,
    pub tz_offset: i64,
    pub modality: Modality,
    pub fit_sport: Option<i64>,
    pub fit_sub_sport: Option<i64>,
    pub name: Option<String>,
    pub elapsed_s: Option<f64>,
    pub timer_s: Option<f64>,
    pub moving_s: Option<f64>,
    /// Distance from the file (or typed in, for manual activities).
    pub distance_m: Option<f64>,
    /// Corrected distance (treadmill), when the user typed one.
    pub distance_override_m: Option<f64>,
    pub calories: Option<f64>,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub min_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub max_cadence: Option<f64>,
    pub avg_speed: Option<f64>,
    pub max_speed: Option<f64>,
    pub avg_power: Option<f64>,
    pub max_power: Option<f64>,
    pub ascent_m: Option<f64>,
    pub descent_m: Option<f64>,
    pub min_alt: Option<f64>,
    pub max_alt: Option<f64>,
    pub avg_temp: Option<f64>,
    pub max_temp: Option<f64>,
    pub training_effect: Option<f64>,
    pub anaerobic_te: Option<f64>,
    pub device: Option<String>,
    pub has_gps: bool,
    pub incline_pct: Option<f64>,
    pub notes: Option<String>,
    pub rpe: Option<i64>,
    pub sleep_quality: Option<i64>,
    pub fatigue: Option<i64>,
    pub soreness: Option<i64>,
    pub body_weight: Option<f64>,
    /// Derived (activity_metrics): training load of the activity.
    pub load: Option<f64>,
    pub drift_pct: Option<f64>,
}

impl Activity {
    /// Distance used everywhere: the user's correction wins over the file.
    pub fn distance(&self) -> Option<f64> {
        self.distance_override_m.or(self.distance_m)
    }

    /// Duration used for pace and totals: moving time, then timer, then elapsed.
    pub fn duration(&self) -> Option<f64> {
        self.moving_s.or(self.timer_s).or(self.elapsed_s)
    }

    /// Average pace in seconds per km (runs only).
    pub fn pace_s_per_km(&self) -> Option<f64> {
        let d = self.distance()?;
        let t = self.duration()?;
        (d > 50.0 && t > 0.0).then(|| t / (d / 1000.0))
    }

    pub fn local_start(&self) -> chrono::NaiveDateTime {
        chrono::DateTime::from_timestamp(self.start_time + self.tz_offset, 0)
            .unwrap_or_default()
            .naive_utc()
    }

    pub fn title(&self) -> String {
        match &self.name {
            Some(n) if !n.trim().is_empty() => n.clone(),
            _ => self.modality.label().to_string(),
        }
    }
}
