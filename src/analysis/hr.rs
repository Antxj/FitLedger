//! Heart-rate based metrics: time in zones, cardiac drift, training load
//! (TRIMP), aerobic efficiency and pace × HR points.

use super::stream::Stream;
use crate::settings::Zones;
use serde::{Deserialize, Serialize};

/// Seconds below zone 1 (index 0) and in zones 1..5.
pub fn time_in_zones(st: &Stream, zones: &Zones) -> [f64; 6] {
    let mut out = [0.0; 6];
    for i in 0..st.len() {
        if let Some(h) = st.hr[i] {
            out[zones.zone_of(h)] += st.dt[i];
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drift {
    pub hr1: f64,
    pub hr2: f64,
    /// Average speed (m/s) of each half.
    pub speed1: Option<f64>,
    pub speed2: Option<f64>,
    /// (hr2 - hr1) / hr1, in %.
    pub hr_pct: f64,
    /// Pa:HR decoupling: loss of speed/HR between halves, in %.
    /// Positive = less efficient in the second half.
    pub decoupling_pct: Option<f64>,
    /// Seconds skipped at the start (warm-up).
    pub skipped_s: f64,
}

/// Compares the first and second half of the moving time.
/// Needs at least 10 minutes with heart rate.
pub fn cardiac_drift(st: &Stream, skip_s: f64) -> Option<Drift> {
    let total = st.moving_time();
    if total - skip_s < 600.0 {
        return None;
    }
    let half = skip_s + (total - skip_s) / 2.0;
    let mut acc = 0.0;
    let mut i_skip = None;
    let mut i_half = None;
    for i in 0..st.len() {
        if st.moving[i] {
            acc += st.dt[i];
        }
        if i_skip.is_none() && acc >= skip_s {
            i_skip = Some(i);
        }
        if acc >= half {
            i_half = Some(i);
            break;
        }
    }
    let (a, m) = (i_skip?, i_half?);
    let hr1 = st.avg(&st.hr, a, m)?;
    let hr2 = st.avg(&st.hr, m, st.len())?;
    let speed1 = st.avg(&st.speed, a, m);
    let speed2 = st.avg(&st.speed, m, st.len());
    let decoupling_pct = match (speed1, speed2) {
        (Some(s1), Some(s2)) if s1 > 0.5 && s2 > 0.5 => {
            let e1 = s1 / hr1;
            let e2 = s2 / hr2;
            Some((e1 - e2) / e1 * 100.0)
        }
        _ => None,
    };
    Some(Drift {
        hr1,
        hr2,
        speed1,
        speed2,
        hr_pct: (hr2 - hr1) / hr1 * 100.0,
        decoupling_pct,
        skipped_s: skip_s,
    })
}

/// Banister TRIMP: minutes × HRr × k × e^(b × HRr), HRr = (HR − rest) / (max − rest).
/// Men: k = 0.64, b = 1.92; women: k = 0.86, b = 1.67 (also used when unknown:
/// men's coefficients).
pub fn trimp(st: &Stream, hr_rest: f64, hr_max: f64, female: bool) -> Option<f64> {
    let (k, b) = if female { (0.86, 1.67) } else { (0.64, 1.92) };
    if hr_max <= hr_rest + 20.0 {
        return None;
    }
    let mut total = 0.0;
    let mut seen = 0.0;
    for i in 0..st.len() {
        if let Some(h) = st.hr[i] {
            let r = ((h - hr_rest) / (hr_max - hr_rest)).clamp(0.0, 1.1);
            total += st.dt[i] / 60.0 * r * k * (b * r).exp();
            seen += st.dt[i];
        }
    }
    (seen >= 60.0).then_some(total)
}

/// Load for activities without heart rate: session RPE (RPE × minutes),
/// scaled to roughly match TRIMP values. With no RPE, a moderate effort is assumed.
pub fn load_without_hr(minutes: f64, rpe: Option<i64>) -> f64 {
    let rpe = rpe.map(|r| r.clamp(1, 10) as f64).unwrap_or(5.0);
    minutes * rpe * 0.3
}

/// Aerobic efficiency: meters per minute per heartbeat (higher is better).
pub fn efficiency(st: &Stream) -> Option<f64> {
    let hr = st.avg(&st.hr, 0, st.len())?;
    let sp = st.avg(&st.speed, 0, st.len())?;
    (hr > 60.0 && sp > 0.5).then(|| sp * 60.0 / hr)
}

/// Steady stretches for the pace × HR analysis: non-overlapping 60 s windows
/// with stable speed (variation below 5%), after the first 3 minutes
/// (heart rate still rising). Returns (pace s/km, average HR).
pub fn pace_hr_points(st: &Stream) -> Vec<[f64; 2]> {
    const WINDOW: f64 = 60.0;
    const SKIP: f64 = 180.0;
    let mut out = Vec::new();
    let n = st.len();
    let mut i = 0;
    while i < n {
        if st.t[i] < SKIP {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut speeds = Vec::new();
        let mut hrs = Vec::new();
        let mut ok = true;
        while j < n && st.t[j] - st.t[i] < WINDOW {
            if !st.moving[j] && j > i {
                ok = false;
                break;
            }
            match (st.speed[j], st.hr[j]) {
                (Some(v), Some(h)) => {
                    speeds.push(v);
                    hrs.push(h);
                }
                _ => {
                    ok = false;
                    break;
                }
            }
            j += 1;
        }
        if !ok || speeds.len() < 40 || j >= n {
            i = j.max(i + 1);
            continue;
        }
        let mean = speeds.iter().sum::<f64>() / speeds.len() as f64;
        let var = speeds.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / speeds.len() as f64;
        if mean > 1.2 && var.sqrt() / mean < 0.05 {
            let hr = hrs.iter().sum::<f64>() / hrs.len() as f64;
            out.push([1000.0 / mean, hr]);
        }
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Modality, Sample};
    use crate::settings::Settings;

    fn run(n: usize, speed: impl Fn(usize) -> f64, hr: impl Fn(usize) -> f64) -> Stream {
        let mut d = 0.0;
        let samples: Vec<Sample> = (0..n)
            .map(|i| {
                let v = speed(i);
                if i > 0 {
                    d += v;
                }
                Sample {
                    t: i as i64,
                    speed: Some(v),
                    distance: Some(d),
                    hr: Some(hr(i)),
                    ..Default::default()
                }
            })
            .collect();
        Stream::build(&samples, Modality::RoadRun, 1.0)
    }

    #[test]
    fn zones_add_up_to_the_recorded_time() {
        let st = run(1801, |_| 2.8, |i| if i < 900 { 130.0 } else { 175.0 });
        let z = Settings {
            hr_max: Some(200.0),
            zone_method: crate::settings::ZoneMethod::MaxHr,
            ..Default::default()
        }
        .zones(None);
        let t = time_in_zones(&st, &z);
        assert!((t.iter().sum::<f64>() - 1800.0).abs() < 1e-6);
        assert!((t[2] - 899.0).abs() <= 1.0, "{t:?}"); // 130 → zone 2 (120-139)
        assert!((t[4] - 900.0).abs() <= 1.0, "{t:?}"); // 175 → zone 4 (160-179)
    }

    #[test]
    fn drift_of_a_steady_run_with_rising_hr() {
        let st = run(1801, |_| 2.78, |i| 160.0 + 10.0 * (i as f64 / 1800.0));
        let d = cardiac_drift(&st, 0.0).unwrap();
        assert!((d.hr1 - 162.5).abs() < 0.5, "{}", d.hr1);
        assert!((d.hr2 - 167.5).abs() < 0.5, "{}", d.hr2);
        assert!(d.hr_pct > 2.5 && d.hr_pct < 3.5);
        // same speed, higher HR → positive decoupling ≈ hr_pct
        let dec = d.decoupling_pct.unwrap();
        assert!(dec > 2.5 && dec < 3.5, "{dec}");
        // too short
        let short = run(400, |_| 2.78, |_| 150.0);
        assert!(cardiac_drift(&short, 0.0).is_none());
    }

    #[test]
    fn trimp_grows_with_intensity_and_time() {
        let easy = run(1801, |_| 2.5, |_| 130.0);
        let hard = run(1801, |_| 3.2, |_| 175.0);
        let long = run(3601, |_| 2.5, |_| 130.0);
        let e = trimp(&easy, 60.0, 190.0, false).unwrap();
        let h = trimp(&hard, 60.0, 190.0, false).unwrap();
        let l = trimp(&long, 60.0, 190.0, false).unwrap();
        // the women's curve gives a different (not wildly different) load
        let hf = trimp(&hard, 60.0, 190.0, true).unwrap();
        assert!((hf / h - 1.0).abs() < 0.2, "{hf} vs {h}");
        assert!(h > e * 1.8, "{e} {h}");
        assert!((l / e - 2.0).abs() < 0.01);
        // 30 min at HRr ≈ 0.885 → ≈ 93
        assert!(h > 80.0 && h < 110.0, "{h}");
    }

    #[test]
    fn pace_hr_points_only_from_steady_parts() {
        // steady 6:00/km for 20 min, then alternating 1-minute surges
        let st = run(
            2401,
            |i| {
                if i < 1200 {
                    1000.0 / 360.0
                } else if (i / 30) % 2 == 0 {
                    2.0
                } else {
                    4.0
                }
            },
            |_| 165.0,
        );
        let pts = pace_hr_points(&st);
        assert!(pts.len() >= 15, "{}", pts.len());
        for p in &pts {
            assert!((p[0] - 360.0).abs() < 1.0);
            assert_eq!(p[1], 165.0);
        }
    }

    #[test]
    fn load_without_hr_scale() {
        assert_eq!(load_without_hr(60.0, Some(5)), 90.0);
        assert_eq!(load_without_hr(60.0, None), 90.0);
        assert_eq!(load_without_hr(60.0, Some(15)), 180.0);
    }
}
