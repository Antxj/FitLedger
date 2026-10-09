//! Splits by distance (500 m, 1 km, 1 mile or any distance) and best efforts.

use super::stream::Stream;

pub const MILE_M: f64 = 1609.344;

#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    /// 1-based number.
    pub n: usize,
    /// Distance covered in this split (the last one may be shorter).
    pub distance_m: f64,
    pub time_s: f64,
    pub pace_s_km: f64,
    pub avg_hr: Option<f64>,
    pub max_hr: Option<f64>,
    pub avg_cadence: Option<f64>,
    pub gain_m: f64,
    pub loss_m: f64,
    /// Elapsed seconds at the end of the split (for charts).
    pub end_s: f64,
}

/// Time (seconds since start) when the accumulated distance reached `d`,
/// interpolated between samples.
fn time_at_distance(st: &Stream, d: f64, from: usize) -> Option<(f64, usize)> {
    let mut prev: Option<(f64, f64)> = None;
    for i in from..st.len() {
        let Some(di) = st.dist[i] else { continue };
        if di >= d {
            let t = match prev {
                Some((pd, pt)) if di > pd => pt + (st.t[i] - pt) * (d - pd) / (di - pd),
                _ => st.t[i],
            };
            return Some((t, i));
        }
        prev = Some((di, st.t[i]));
    }
    None
}

/// Moving time between two elapsed instants (pauses inside a split do not count).
fn moving_between(st: &Stream, t_from: f64, t_to: f64) -> f64 {
    let mut total = 0.0;
    for i in 1..st.len() {
        if !st.moving[i] {
            continue;
        }
        let (a, b) = (st.t[i] - st.dt[i], st.t[i]);
        let lo = a.max(t_from);
        let hi = b.min(t_to);
        if hi > lo {
            total += hi - lo;
        }
    }
    total
}

pub fn splits(st: &Stream, step_m: f64) -> Vec<Split> {
    let mut out = Vec::new();
    let Some(total) = st.total_distance() else { return out };
    if step_m < 50.0 || total < 1.0 {
        return out;
    }
    let mut start_t = 0.0;
    let mut start_i = 0;
    let mut start_d = 0.0;
    let mut n = 1;
    loop {
        let target = (n as f64 * step_m).min(total);
        let Some((t_end, i_end)) = time_at_distance(st, target, start_i) else {
            break;
        };
        let seg_d = target - start_d;
        // a last bit of a few meters (watch stopped just after the mark) is not a split
        if seg_d < 50.0 {
            break;
        }
        let time = moving_between(st, start_t, t_end).max(0.001);
        let end_idx = (i_end + 1).min(st.len());
        let (gain, loss) = st.elevation(start_i, end_idx);
        out.push(Split {
            n,
            distance_m: seg_d,
            time_s: time,
            pace_s_km: time / (seg_d / 1000.0),
            avg_hr: st.avg(&st.hr, start_i, end_idx),
            max_hr: st.max(&st.hr, start_i, end_idx),
            avg_cadence: st.avg(&st.cadence, start_i, end_idx),
            gain_m: gain,
            loss_m: loss,
            end_s: t_end,
        });
        if target >= total {
            break;
        }
        start_t = t_end;
        start_i = i_end;
        start_d = target;
        n += 1;
    }
    out
}

/// Fastest time to cover `d` meters anywhere in the activity (elapsed time,
/// interpolated). Returns (time, start seconds).
pub fn best_effort(st: &Stream, d: f64) -> Option<(f64, f64)> {
    let pts: Vec<(f64, f64)> =
        st.t.iter()
            .zip(&st.dist)
            .filter_map(|(t, d)| d.map(|d| (*t, d)))
            .collect();
    if pts.len() < 2 || pts.last()?.1 - pts[0].1 < d {
        return None;
    }
    let mut best: Option<(f64, f64)> = None;
    let mut j = 0;
    for i in 0..pts.len() {
        if j < i {
            j = i;
        }
        while j < pts.len() && pts[j].1 - pts[i].1 < d {
            j += 1;
        }
        if j >= pts.len() {
            break;
        }
        // interpolate the exact instant where the distance d is reached
        let (tj, dj) = pts[j];
        let (tp, dp) = pts[j - 1];
        let need = pts[i].1 + d;
        let t_reach = if dj > dp {
            tp + (tj - tp) * (need - dp) / (dj - dp)
        } else {
            tj
        };
        let time = t_reach - pts[i].0;
        if time > 0.0 && best.is_none_or(|(b, _)| time < b) {
            best = Some((time, pts[i].0));
        }
    }
    best
}

/// Distances of the records page.
pub const RECORD_DISTANCES: [f64; 6] = [400.0, 1000.0, MILE_M, 3000.0, 5000.0, 10000.0];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Modality, Sample};

    fn steady(n: usize, speed: f64) -> Stream {
        let samples: Vec<Sample> = (0..n)
            .map(|i| Sample {
                t: i as i64,
                speed: Some(speed),
                distance: Some(i as f64 * speed),
                hr: Some(140.0 + i as f64 / 60.0),
                cadence: Some(170.0),
                ..Default::default()
            })
            .collect();
        Stream::build(&samples, Modality::RoadRun, 1.0)
    }

    #[test]
    fn km_splits_with_partial_last() {
        // 10 km/h = 2.7778 m/s → 6:00/km, 2.5 km
        let st = steady(901, 1000.0 / 360.0);
        let sp = splits(&st, 1000.0);
        assert_eq!(sp.len(), 3);
        for s in &sp[..2] {
            assert!((s.pace_s_km - 360.0).abs() < 1.0, "{}", s.pace_s_km);
            assert!((s.distance_m - 1000.0).abs() < 1e-6);
        }
        assert!((sp[2].distance_m - 500.0).abs() < 1.0);
        assert!((sp[2].pace_s_km - 360.0).abs() < 1.5);
        assert!(sp[1].avg_hr.unwrap() > sp[0].avg_hr.unwrap());
    }

    #[test]
    fn other_split_distances() {
        let st = steady(3601, 1000.0 / 360.0); // 10 km
        assert_eq!(splits(&st, 500.0).len(), 20);
        assert_eq!(splits(&st, MILE_M).len(), 7); // 6 full miles + 0.21
        assert_eq!(splits(&st, 2500.0).len(), 4);
    }

    #[test]
    fn pauses_do_not_count_in_split_time() {
        let mut samples: Vec<Sample> = (0..601)
            .map(|i| Sample {
                t: i as i64,
                speed: Some(2.5),
                distance: Some(i as f64 * 2.5),
                ..Default::default()
            })
            .collect();
        // a 2-minute gap (watch auto-pause) in the middle of the first km
        for s in samples.iter_mut().skip(200) {
            s.t += 120;
        }
        let st = Stream::build(&samples, Modality::RoadRun, 1.0);
        let sp = splits(&st, 1000.0);
        assert!((sp[0].time_s - 400.0).abs() < 2.0, "{}", sp[0].time_s);
    }

    #[test]
    fn best_effort_finds_the_fast_part() {
        // 1 km slow (3 m/s... no: 2.5) then 1 km fast (4 m/s)
        let mut samples = Vec::new();
        let mut d = 0.0;
        for i in 0..800 {
            let v = if i < 400 { 2.5 } else { 4.0 };
            if i > 0 {
                d += v;
            }
            samples.push(Sample {
                t: i,
                speed: Some(v),
                distance: Some(d),
                ..Default::default()
            });
        }
        let st = Stream::build(&samples, Modality::RoadRun, 1.0);
        let (t, start) = best_effort(&st, 1000.0).unwrap();
        assert!((t - 250.0).abs() < 1.0, "{t}");
        assert!(start >= 399.0);
        assert!(best_effort(&st, 10_000.0).is_none());
    }
}
