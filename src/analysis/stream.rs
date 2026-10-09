//! Cleaned per-second stream of an activity: the input of every metric.
//!
//! Bad readings are never deleted from the database. Here they are only
//! flagged and replaced by None, so the metrics ignore them (item 38).

use crate::model::{Modality, Sample};
use serde::{Deserialize, Serialize};

/// Samples further apart than this are a pause (or a recording gap).
pub const MAX_GAP_S: f64 = 10.0;
/// Below this speed a runner is considered stopped (≈ 20:50/km).
pub const MOVING_SPEED: f64 = 0.8;
/// Fastest plausible speed on foot (≈ 43 km/h).
const MAX_RUN_SPEED: f64 = 12.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QualityReport {
    /// Heart rate 0, out of 30–230 or an isolated spike.
    pub hr: u32,
    /// Impossible speed or abnormal instantaneous pace.
    pub speed: u32,
    /// GPS jumps.
    pub gps: u32,
    /// Isolated wrong cadence.
    pub cadence: u32,
    /// Accumulated distance going backwards / jumping.
    pub distance: u32,
}

impl QualityReport {
    pub fn total(&self) -> u32 {
        self.hr + self.speed + self.gps + self.cadence + self.distance
    }
}

#[derive(Debug, Clone, Default)]
pub struct Stream {
    /// Seconds since the first sample.
    pub t: Vec<f64>,
    /// Unix time of the first sample.
    pub t0: i64,
    pub dist: Vec<Option<f64>>,
    pub hr: Vec<Option<f64>>,
    pub speed: Vec<Option<f64>>,
    pub cadence: Vec<Option<f64>>,
    pub alt: Vec<Option<f64>>,
    pub power: Vec<Option<f64>>,
    pub temp: Vec<Option<f64>>,
    pub pos: Vec<Option<(f64, f64)>>,
    /// Time this sample represents (gap to the previous one, capped; 0 in pauses).
    pub dt: Vec<f64>,
    pub moving: Vec<bool>,
    pub quality: QualityReport,
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(v[v.len() / 2])
}

/// Rolling median of the valid values around each index (window = 2*half+1).
fn rolling_median(x: &[Option<f64>], half: usize) -> Vec<Option<f64>> {
    let n = x.len();
    let mut out = Vec::with_capacity(n);
    let mut buf = Vec::with_capacity(2 * half + 1);
    for i in 0..n {
        buf.clear();
        let lo = i.saturating_sub(half);
        let hi = (i + half + 1).min(n);
        buf.extend(x[lo..hi].iter().flatten().copied());
        out.push(median(&mut buf));
    }
    out
}

/// Great-circle distance in meters.
pub fn haversine(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = 6_371_000.0;
    let (la1, lo1, la2, lo2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * r * h.sqrt().asin()
}

impl Stream {
    /// `distance_scale`: correction factor for the distance (treadmill distance
    /// typed by the user / distance recorded by the watch).
    pub fn build(samples: &[Sample], modality: Modality, distance_scale: f64) -> Stream {
        let n = samples.len();
        let mut s = Stream {
            t0: samples.first().map(|x| x.t).unwrap_or(0),
            ..Default::default()
        };
        if n == 0 {
            return s;
        }
        let is_run = modality.is_run();
        let scale = if distance_scale.is_finite() && distance_scale > 0.0 {
            distance_scale
        } else {
            1.0
        };
        s.t = samples.iter().map(|x| (x.t - s.t0) as f64).collect();
        s.alt = samples.iter().map(|x| x.alt).collect();
        s.power = samples
            .iter()
            .map(|x| x.power.filter(|&p| p > 0.0 && p < 2500.0))
            .collect();
        s.temp = samples
            .iter()
            .map(|x| x.temp.filter(|&v| (-40.0..=60.0).contains(&v)))
            .collect();

        // --- heart rate: absurd values and isolated spikes
        let raw_hr: Vec<Option<f64>> = samples.iter().map(|x| x.hr).collect();
        let hr_med = rolling_median(&raw_hr, 5);
        s.hr = raw_hr
            .iter()
            .zip(&hr_med)
            .map(|(h, m)| {
                let h = (*h)?;
                if !(30.0..=230.0).contains(&h) {
                    s.quality.hr += 1;
                    return None;
                }
                if let Some(m) = m
                    && (h - m).abs() > 35.0
                {
                    s.quality.hr += 1;
                    return None;
                }
                Some(h)
            })
            .collect();

        // --- GPS: jumps faster than any runner
        s.pos = Vec::with_capacity(n);
        let mut last_good: Option<(f64, f64, f64)> = None; // lat, lon, t
        for (i, x) in samples.iter().enumerate() {
            let p = match (x.lat, x.lon) {
                (Some(a), Some(b)) => Some((a, b)),
                _ => None,
            };
            let p = p.filter(|&p| {
                if let Some((la, lo, t)) = last_good {
                    let dt = (s.t[i] - t).max(1.0);
                    let v = haversine((la, lo), p) / dt;
                    if is_run && v > MAX_RUN_SPEED * 1.5 {
                        s.quality.gps += 1;
                        return false;
                    }
                }
                true
            });
            if let Some((a, b)) = p {
                last_good = Some((a, b, s.t[i]));
            }
            s.pos.push(p);
        }

        // --- distance: from the file; else integrated from GPS or speed
        let has_dist = samples.iter().any(|x| x.distance.is_some_and(|d| d > 0.0));
        let mut dist: Vec<Option<f64>> = Vec::with_capacity(n);
        if has_dist {
            let mut prev: Option<f64> = None;
            for x in samples {
                let d = x.distance.filter(|&d| d >= 0.0);
                let d = match (d, prev) {
                    (Some(d), Some(p)) if d + 0.5 < p => {
                        s.quality.distance += 1;
                        Some(p)
                    }
                    (Some(d), _) => Some(d),
                    (None, p) => p,
                };
                if d.is_some() {
                    prev = d;
                }
                dist.push(d);
            }
        } else if s.pos.iter().flatten().count() > 10 {
            let mut acc = 0.0;
            let mut last: Option<(f64, f64)> = None;
            for p in &s.pos {
                if let (Some(p), Some(l)) = (p, last) {
                    acc += haversine(l, *p);
                }
                if p.is_some() {
                    last = *p;
                }
                dist.push(Some(acc));
            }
        } else if samples.iter().any(|x| x.speed.is_some_and(|v| v > 0.0)) {
            let mut acc = 0.0;
            for (i, x) in samples.iter().enumerate() {
                if i > 0 {
                    let dt = (s.t[i] - s.t[i - 1]).min(MAX_GAP_S);
                    acc += x.speed.unwrap_or(0.0).max(0.0) * dt;
                }
                dist.push(Some(acc));
            }
        } else {
            dist = vec![None; n];
        }
        s.dist = dist.into_iter().map(|d| d.map(|d| d * scale)).collect();

        // --- speed: from the file, else from distance; impossible values out
        let mut speed: Vec<Option<f64>> = samples.iter().map(|x| x.speed.map(|v| v * scale)).collect();
        if speed.iter().all(Option::is_none) {
            #[allow(clippy::needless_range_loop)] // reads three parallel arrays
            for i in 1..n {
                if let (Some(a), Some(b)) = (s.dist[i - 1], s.dist[i]) {
                    let dt = s.t[i] - s.t[i - 1];
                    if dt > 0.0 && dt <= MAX_GAP_S {
                        speed[i] = Some((b - a) / dt);
                    }
                }
            }
        }
        let sp_med = rolling_median(&speed, 7);
        s.speed = speed
            .iter()
            .zip(&sp_med)
            .map(|(v, m)| {
                let v = (*v)?;
                if v < 0.0 || (is_run && v > MAX_RUN_SPEED) {
                    s.quality.speed += 1;
                    return None;
                }
                if is_run && let Some(m) = m {
                    // abnormal instantaneous pace: isolated jump against the neighbours
                    if (v - *m).abs() > 2.5 && *m > MOVING_SPEED {
                        s.quality.speed += 1;
                        return None;
                    }
                }
                Some(v)
            })
            .collect();

        // --- cadence: isolated wrong values (runs)
        let raw_cad: Vec<Option<f64>> = samples.iter().map(|x| x.cadence).collect();
        let cad_med = rolling_median(&raw_cad, 7);
        s.cadence = raw_cad
            .iter()
            .zip(&cad_med)
            .zip(&s.speed)
            .map(|((c, m), v)| {
                let c = (*c)?;
                if !is_run {
                    return Some(c).filter(|&c| c > 0.0);
                }
                let moving = v.is_some_and(|v| v > 1.5);
                if c <= 0.0 {
                    if moving {
                        s.quality.cadence += 1;
                    }
                    return None;
                }
                if c > 260.0 || m.is_some_and(|m| (c - m).abs() > 40.0) {
                    s.quality.cadence += 1;
                    return None;
                }
                Some(c)
            })
            .collect();

        // --- moving time and pauses
        s.dt = Vec::with_capacity(n);
        s.moving = Vec::with_capacity(n);
        let has_speed = s.speed.iter().any(Option::is_some);
        for i in 0..n {
            let gap = if i == 0 { 0.0 } else { s.t[i] - s.t[i - 1] };
            let dt = if gap <= MAX_GAP_S { gap } else { 0.0 };
            let moving = if has_speed && modality.is_run() {
                s.speed[i].is_some_and(|v| v >= MOVING_SPEED)
            } else {
                true
            };
            s.dt.push(dt);
            s.moving.push(moving && dt > 0.0);
        }
        s
    }

    pub fn len(&self) -> usize {
        self.t.len()
    }

    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }

    pub fn moving_time(&self) -> f64 {
        self.dt
            .iter()
            .zip(&self.moving)
            .filter(|(_, m)| **m)
            .map(|(d, _)| d)
            .sum()
    }

    pub fn total_distance(&self) -> Option<f64> {
        self.dist.iter().rev().flatten().next().copied()
    }

    /// Pauses: (start seconds, duration seconds). Gaps between samples and
    /// stretches stopped for 10 s or more.
    pub fn pauses(&self) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        let mut i = 1;
        while i < self.len() {
            let gap = self.t[i] - self.t[i - 1];
            if gap > MAX_GAP_S {
                out.push((self.t[i - 1], gap));
                i += 1;
                continue;
            }
            if !self.moving[i] {
                let start = self.t[i - 1];
                let mut j = i;
                while j < self.len() && !self.moving[j] && self.t[j] - self.t[j - 1] <= MAX_GAP_S {
                    j += 1;
                }
                let end = self.t[j.min(self.len() - 1)];
                if end - start >= MAX_GAP_S {
                    out.push((start, end - start));
                }
                i = j.max(i + 1);
                continue;
            }
            i += 1;
        }
        out
    }

    /// Time-weighted average of a series over [from, to) indexes, moving samples only.
    pub fn avg(&self, series: &[Option<f64>], from: usize, to: usize) -> Option<f64> {
        let (mut sum, mut w) = (0.0, 0.0);
        #[allow(clippy::needless_range_loop)] // reads three parallel arrays
        for i in from..to.min(self.len()) {
            if let Some(v) = series[i] {
                let dt = if self.dt[i] > 0.0 { self.dt[i] } else { 1.0 };
                if self.moving[i] || i == from {
                    sum += v * dt;
                    w += dt;
                }
            }
        }
        (w > 0.0).then(|| sum / w)
    }

    pub fn max(&self, series: &[Option<f64>], from: usize, to: usize) -> Option<f64> {
        series[from..to.min(self.len())]
            .iter()
            .flatten()
            .copied()
            .reduce(f64::max)
    }

    /// Elevation gain and loss with a 3 m hysteresis (ignores barometer noise).
    pub fn elevation(&self, from: usize, to: usize) -> (f64, f64) {
        let mut gain = 0.0;
        let mut loss = 0.0;
        let mut anchor: Option<f64> = None;
        for v in self.alt[from..to.min(self.len())].iter().flatten() {
            match anchor {
                None => anchor = Some(*v),
                Some(a) => {
                    if v - a >= 3.0 {
                        gain += v - a;
                        anchor = Some(*v);
                    } else if a - v >= 3.0 {
                        loss += a - v;
                        anchor = Some(*v);
                    }
                }
            }
        }
        (gain, loss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(n: usize, f: impl Fn(usize, &mut Sample)) -> Vec<Sample> {
        (0..n)
            .map(|i| {
                let mut s = Sample {
                    t: 1000 + i as i64,
                    hr: Some(150.0),
                    speed: Some(2.5),
                    distance: Some(i as f64 * 2.5),
                    cadence: Some(170.0),
                    alt: Some(100.0),
                    ..Default::default()
                };
                f(i, &mut s);
                s
            })
            .collect()
    }

    #[test]
    fn flags_bad_readings_without_touching_the_rest() {
        let samples = run(300, |i, s| match i {
            50 => s.hr = Some(0.0),
            60 => s.hr = Some(250.0),
            70 => s.hr = Some(210.0), // spike against 150 around it
            80 => s.speed = Some(30.0),
            90 => s.cadence = Some(90.0),
            100 => s.cadence = Some(0.0),
            _ => {}
        });
        let st = Stream::build(&samples, Modality::RoadRun, 1.0);
        assert_eq!(st.quality.hr, 3);
        assert_eq!(st.quality.speed, 1);
        assert_eq!(st.quality.cadence, 2);
        assert_eq!(st.hr[50], None);
        assert_eq!(st.hr[51], Some(150.0));
        assert_eq!(st.avg(&st.hr, 0, 300), Some(150.0));
        // original samples are untouched
        assert_eq!(samples[50].hr, Some(0.0));
    }

    #[test]
    fn gps_jump_is_flagged() {
        let samples = run(100, |i, s| {
            let lon = if i == 40 { 1.0 } else { i as f64 * 2.5 / 111_320.0 };
            s.lat = Some(0.01);
            s.lon = Some(lon);
        });
        let st = Stream::build(&samples, Modality::RoadRun, 1.0);
        assert_eq!(st.quality.gps, 1);
        assert!(st.pos[40].is_none());
    }

    #[test]
    fn moving_time_and_pauses() {
        // 0..100 running, 100..160 stopped, 160..300 running, then a 60 s gap
        let mut samples = run(300, |i, s| {
            if (100..160).contains(&i) {
                s.speed = Some(0.0);
            }
        });
        for s in samples.iter_mut().skip(250) {
            s.t += 60;
        }
        let st = Stream::build(&samples, Modality::RoadRun, 1.0);
        let p = st.pauses();
        assert_eq!(p.len(), 2, "{p:?}");
        assert!((p[0].1 - 60.0).abs() <= 1.0);
        assert!((p[1].1 - 61.0).abs() <= 1.0);
        let moving = st.moving_time();
        assert!((moving - 238.0).abs() <= 2.0, "{moving}");
    }

    #[test]
    fn distance_correction_scales_everything() {
        let samples = run(401, |_, _| {});
        let st = Stream::build(&samples, Modality::Treadmill, 1.1);
        assert!((st.total_distance().unwrap() - 1100.0).abs() < 0.01);
        assert!((st.speed[10].unwrap() - 2.75).abs() < 1e-9);
    }

    #[test]
    fn elevation_ignores_noise() {
        let samples = run(200, |i, s| {
            s.alt = Some(100.0 + if i % 2 == 0 { 1.0 } else { 0.0 } + if i > 100 { 20.0 } else { 0.0 });
        });
        let st = Stream::build(&samples, Modality::RoadRun, 1.0);
        let (g, l) = st.elevation(0, 200);
        assert!((g - 20.0).abs() <= 1.0, "{g}");
        assert!(l <= 1.0, "{l}");
    }

    #[test]
    fn haversine_one_degree_at_equator() {
        assert!((haversine((0.0, 0.0), (0.0, 1.0)) - 111_195.0).abs() < 10.0);
    }
}
