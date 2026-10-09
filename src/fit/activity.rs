//! FIT profile → FitLedger model. Only the messages and fields we use are
//! interpreted; everything else stays available in the original file kept
//! in the database.

use super::decode::{Message, TIMESTAMP, Value};
use crate::model::{Lap, Modality, Sample, StrengthSet, TimerEvent};

/// Seconds between the Unix epoch and the FIT epoch (1989-12-31 00:00 UTC).
pub const FIT_EPOCH: i64 = 631_065_600;

mod msg {
    pub const FILE_ID: u16 = 0;
    pub const SPORT: u16 = 12;
    pub const SESSION: u16 = 18;
    pub const LAP: u16 = 19;
    pub const RECORD: u16 = 20;
    pub const EVENT: u16 = 21;
    pub const DEVICE_INFO: u16 = 23;
    pub const ACTIVITY: u16 = 34;
    pub const SET: u16 = 225;
}

/// Everything extracted from one FIT activity file.
#[derive(Debug, Clone, Default)]
pub struct ParsedActivity {
    pub start_time: i64,
    /// Local time minus UTC, in seconds, as recorded by the device.
    pub tz_offset: Option<i64>,
    pub fit_sport: Option<i64>,
    pub fit_sub_sport: Option<i64>,
    pub sport_name: Option<String>,
    pub modality: Modality,
    pub elapsed_s: Option<f64>,
    pub timer_s: Option<f64>,
    pub distance_m: Option<f64>,
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
    pub device_serial: Option<i64>,
    pub time_created: Option<i64>,
    pub laps: Vec<Lap>,
    pub samples: Vec<Sample>,
    pub events: Vec<TimerEvent>,
    pub sets: Vec<StrengthSet>,
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    NotActivity,
    NoData,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ParseError::NotActivity => tr!(
                "o arquivo não é de uma atividade (pode ser de configurações, cursos ou monitoramento)",
                "the file is not an activity (it may hold settings, courses or monitoring data)"
            ),
            ParseError::NoData => tr!(
                "a atividade não tem horário nem registros",
                "the activity has no start time and no records"
            ),
        })
    }
}

fn f(m: &Message, field: u8) -> Option<f64> {
    m.get(field).and_then(Value::as_f64)
}

fn i(m: &Message, field: u8) -> Option<i64> {
    m.get(field).and_then(Value::as_i64)
}

fn scaled(m: &Message, field: u8, scale: f64, offset: f64) -> Option<f64> {
    f(m, field).map(|v| v / scale - offset)
}

fn time(m: &Message, field: u8) -> Option<i64> {
    // values below 0x10000000 are relative times (seconds since device power-on)
    i(m, field).filter(|&t| t >= 0x1000_0000).map(|t| t + FIT_EPOCH)
}

fn text(m: &Message, field: u8) -> Option<String> {
    match m.get(field) {
        Some(Value::Text(s)) => Some(s.clone()),
        _ => None,
    }
}

fn semicircles(m: &Message, field: u8) -> Option<f64> {
    f(m, field).map(|v| v * (180.0 / 2f64.powi(31))).filter(|d| d.abs() <= 180.0 && *d != 0.0)
}

/// Running cadence in FIT is in strides (cycles) per minute; we show steps.
fn is_running_sport(sport: Option<i64>) -> bool {
    matches!(sport, Some(1) | Some(11)) // running, walking
}

pub fn modality_for(sport: Option<i64>, sub_sport: Option<i64>) -> Modality {
    match (sport, sub_sport) {
        (Some(1), Some(1)) => Modality::Treadmill, // running / treadmill
        (Some(1), Some(45)) => Modality::Treadmill, // running / indoor_running
        (Some(1), _) => Modality::RoadRun,
        (Some(8), _) => Modality::Tennis,
        (Some(4), Some(20)) | (Some(10), Some(20)) => Modality::Strength, // strength_training
        (Some(10), _) => Modality::Strength, // generic training
        _ => Modality::Other,
    }
}

pub fn parse(messages: &[Message]) -> Result<ParsedActivity, ParseError> {
    let mut a = ParsedActivity::default();

    if let Some(fid) = messages.iter().find(|m| m.global == msg::FILE_ID) {
        // type 4 = activity. Missing type: try anyway.
        if let Some(t) = i(fid, 0) {
            if t != 4 {
                return Err(ParseError::NotActivity);
            }
        }
        a.device_serial = i(fid, 3);
        a.time_created = time(fid, 4);
        a.device = text(fid, 8).or_else(|| {
            let manufacturer = i(fid, 1)?;
            Some(format!("{} {}", manufacturer_name(manufacturer), i(fid, 2).unwrap_or(0)))
        });
    }
    if a.device.is_none() {
        a.device = messages
            .iter()
            .filter(|m| m.global == msg::DEVICE_INFO)
            .find_map(|m| text(m, 27));
    }

    let session = messages.iter().find(|m| m.global == msg::SESSION);
    if let Some(sp) = messages.iter().find(|m| m.global == msg::SPORT) {
        a.fit_sport = i(sp, 0);
        a.fit_sub_sport = i(sp, 1);
        a.sport_name = text(sp, 3);
    }
    if let Some(s) = session {
        a.fit_sport = i(s, 5).or(a.fit_sport);
        a.fit_sub_sport = i(s, 6).or(a.fit_sub_sport);
        a.start_time = time(s, 2).unwrap_or(0);
        a.elapsed_s = scaled(s, 7, 1000.0, 0.0);
        a.timer_s = scaled(s, 8, 1000.0, 0.0);
        a.distance_m = scaled(s, 9, 100.0, 0.0).filter(|&d| d > 0.0);
        a.calories = f(s, 11);
        a.avg_speed = scaled(s, 124, 1000.0, 0.0).or_else(|| scaled(s, 14, 1000.0, 0.0));
        a.max_speed = scaled(s, 125, 1000.0, 0.0).or_else(|| scaled(s, 15, 1000.0, 0.0));
        a.avg_hr = f(s, 16);
        a.max_hr = f(s, 17);
        a.min_hr = f(s, 64);
        a.avg_cadence = f(s, 18).map(|c| c + f(s, 92).unwrap_or(0.0) / 128.0);
        a.max_cadence = f(s, 19);
        a.avg_power = f(s, 20);
        a.max_power = f(s, 21);
        a.ascent_m = f(s, 22);
        a.descent_m = f(s, 23);
        a.training_effect = scaled(s, 24, 10.0, 0.0);
        a.anaerobic_te = scaled(s, 137, 10.0, 0.0);
        a.max_alt = scaled(s, 128, 5.0, 500.0).or_else(|| scaled(s, 50, 5.0, 500.0));
        a.min_alt = scaled(s, 127, 5.0, 500.0).or_else(|| scaled(s, 71, 5.0, 500.0));
        a.avg_temp = f(s, 57);
        a.max_temp = f(s, 58);
        if a.sport_name.is_none() {
            a.sport_name = text(s, 110);
        }
    }
    let running = is_running_sport(a.fit_sport);
    if running {
        a.avg_cadence = a.avg_cadence.map(|c| c * 2.0);
        a.max_cadence = a.max_cadence.map(|c| c * 2.0);
    }

    for m in messages {
        match m.global {
            msg::RECORD => {
                let Some(t) = time(m, TIMESTAMP) else { continue };
                let cadence = f(m, 4).map(|c| {
                    let c = c + f(m, 53).unwrap_or(0.0) / 128.0;
                    if running { c * 2.0 } else { c }
                });
                a.samples.push(Sample {
                    t,
                    lat: semicircles(m, 0),
                    lon: semicircles(m, 1),
                    alt: scaled(m, 78, 5.0, 500.0).or_else(|| scaled(m, 2, 5.0, 500.0)),
                    hr: f(m, 3),
                    cadence,
                    distance: scaled(m, 5, 100.0, 0.0),
                    speed: scaled(m, 73, 1000.0, 0.0).or_else(|| scaled(m, 6, 1000.0, 0.0)),
                    power: f(m, 7),
                    temp: f(m, 13),
                });
            }
            msg::LAP => {
                let Some(start) = time(m, 2).or_else(|| time(m, TIMESTAMP)) else { continue };
                a.laps.push(Lap {
                    start,
                    elapsed_s: scaled(m, 7, 1000.0, 0.0),
                    timer_s: scaled(m, 8, 1000.0, 0.0),
                    distance_m: scaled(m, 9, 100.0, 0.0),
                    calories: f(m, 11),
                    avg_speed: scaled(m, 110, 1000.0, 0.0).or_else(|| scaled(m, 13, 1000.0, 0.0)),
                    max_speed: scaled(m, 111, 1000.0, 0.0).or_else(|| scaled(m, 14, 1000.0, 0.0)),
                    avg_hr: f(m, 15),
                    max_hr: f(m, 16),
                    // Some devices write lap cadence already in steps/min; values
                    // below 120 can only be strides/min for a run.
                    avg_cadence: f(m, 17).map(|c| if running && c < 120.0 { c * 2.0 } else { c }),
                    ascent_m: f(m, 21),
                    descent_m: f(m, 22),
                    trigger: i(m, 24),
                });
            }
            msg::EVENT => {
                if let (Some(t), Some(event), Some(event_type)) = (time(m, TIMESTAMP), i(m, 0), i(m, 1)) {
                    a.events.push(TimerEvent { t, event, event_type });
                }
            }
            msg::ACTIVITY => {
                if let (Some(ts), Some(local)) = (i(m, TIMESTAMP), i(m, 5)) {
                    let off = local - ts;
                    // sanity: real offsets are within ±14 h, in 15-minute steps
                    if off.abs() <= 14 * 3600 && off % 900 == 0 {
                        a.tz_offset = Some(off);
                    }
                }
            }
            msg::SET => {
                // only active sets (set_type 1); rest periods are skipped
                if i(m, 5) == Some(1) {
                    a.sets.push(StrengthSet {
                        t: time(m, 6).or_else(|| time(m, TIMESTAMP)),
                        duration_s: scaled(m, 0, 1000.0, 0.0),
                        reps: i(m, 3),
                        weight_kg: scaled(m, 4, 16.0, 0.0),
                        category: match m.get(7) {
                            Some(Value::Array(v)) => v.first().copied().flatten(),
                            Some(v) => v.as_i64(),
                            None => None,
                        },
                    });
                }
            }
            _ => {}
        }
    }
    a.samples.sort_by_key(|s| s.t);
    a.samples.dedup_by_key(|s| s.t);

    if a.start_time == 0 {
        a.start_time = a
            .samples
            .first()
            .map(|s| s.t)
            .or_else(|| a.events.first().map(|e| e.t))
            .ok_or(ParseError::NoData)?;
    }
    fill_from_samples(&mut a);
    a.modality = modality_for(a.fit_sport, a.fit_sub_sport);
    Ok(a)
}

/// Fills summary values the session did not have (or files without a session).
fn fill_from_samples(a: &mut ParsedActivity) {
    let s = &a.samples;
    if s.is_empty() {
        return;
    }
    let first = s.first().unwrap().t;
    let last = s.last().unwrap().t;
    a.elapsed_s.get_or_insert((last - first) as f64);
    if a.timer_s.is_none() {
        a.timer_s = a.elapsed_s;
    }
    let hrs: Vec<f64> = s.iter().filter_map(|x| x.hr).filter(|&h| h > 0.0).collect();
    if !hrs.is_empty() {
        a.avg_hr.get_or_insert(hrs.iter().sum::<f64>() / hrs.len() as f64);
        a.max_hr.get_or_insert(hrs.iter().cloned().fold(f64::MIN, f64::max));
        a.min_hr.get_or_insert(hrs.iter().cloned().fold(f64::MAX, f64::min));
    }
    if a.distance_m.is_none() {
        a.distance_m = s.iter().rev().find_map(|x| x.distance).filter(|&d| d > 0.0);
    }
    let alts: Vec<f64> = s.iter().filter_map(|x| x.alt).collect();
    if !alts.is_empty() {
        a.min_alt.get_or_insert(alts.iter().cloned().fold(f64::MAX, f64::min));
        a.max_alt.get_or_insert(alts.iter().cloned().fold(f64::MIN, f64::max));
    }
    let temps: Vec<f64> = s.iter().filter_map(|x| x.temp).collect();
    if !temps.is_empty() {
        a.avg_temp.get_or_insert(temps.iter().sum::<f64>() / temps.len() as f64);
        a.max_temp.get_or_insert(temps.iter().cloned().fold(f64::MIN, f64::max));
    }
}

fn manufacturer_name(id: i64) -> &'static str {
    match id {
        1 => "Garmin",
        23 => "Suunto",
        32 => "Wahoo",
        255 => "Development",
        265 => "Strava",
        267 => "Bryton",
        294 => "Coros",
        // Zepp/Huami (Amazfit) devices usually write product_name instead
        _ => "Device",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::decode::decode;
    use crate::fit::writer::Synthetic;

    #[test]
    fn synthetic_run_is_parsed() {
        let fit = Synthetic::run(1_100_000_000, 1800, 2.78, 140.0, 160.0, true).encode();
        let a = parse(&decode(&fit).unwrap()).unwrap();
        assert_eq!(a.start_time, 1_100_000_000 + FIT_EPOCH);
        assert_eq!(a.modality, Modality::RoadRun);
        assert_eq!(a.tz_offset, Some(-3 * 3600));
        assert_eq!(a.samples.len(), 1800);
        assert_eq!(a.device.as_deref(), Some("Test Watch 1"));
        assert_eq!(a.device_serial, Some(4242));
        let d = a.distance_m.unwrap();
        assert!((d - 2.78 * 1799.0).abs() < 1.0, "{d}");
        // strides/min → steps/min
        assert_eq!(a.samples[10].cadence, Some(164.0));
        let s = &a.samples[100];
        assert!((s.lat.unwrap() - 0.01).abs() < 1e-6);
        assert!(s.lon.unwrap() > 0.0);
        assert!((s.alt.unwrap() - 100.0).abs() < 6.0);
        assert_eq!(a.events.len(), 2);
        assert_eq!(a.laps.len(), 1);
    }

    #[test]
    fn treadmill_without_gps() {
        let fit = Synthetic::run(1_100_000_000, 600, 2.78, 140.0, 150.0, false).encode();
        let a = parse(&decode(&fit).unwrap()).unwrap();
        assert_eq!(a.modality, Modality::Treadmill);
        assert!(a.samples.iter().all(|s| s.lat.is_none() && s.lon.is_none()));
    }

    #[test]
    fn modalities() {
        assert_eq!(modality_for(Some(1), Some(2)), Modality::RoadRun);
        assert_eq!(modality_for(Some(1), Some(1)), Modality::Treadmill);
        assert_eq!(modality_for(Some(8), Some(0)), Modality::Tennis);
        assert_eq!(modality_for(Some(10), Some(20)), Modality::Strength);
        assert_eq!(modality_for(Some(2), None), Modality::Other);
        assert_eq!(modality_for(None, None), Modality::Other);
    }
}
