//! Automatic insights (item 28). Each one is a simple rule with a minimum of
//! data, and always carries the evidence that led to it.

use super::load::day_of;
use super::metrics::Metrics;
use super::trends;
use crate::format;
use crate::model::{Activity, Modality};
use chrono::{Duration, NaiveDate};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Good,
    Neutral,
    Attention,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Insight {
    pub text: String,
    pub tone: Tone,
    /// The data behind the conclusion, one line per item.
    pub evidence: Vec<String>,
}

pub struct Input<'a> {
    pub acts: &'a [Activity],
    pub metrics: &'a HashMap<i64, Metrics>,
    /// (test name, runs of the test, oldest first).
    pub ref_tests: Vec<(String, Vec<i64>)>,
    pub today: NaiveDate,
}

fn runs<'a>(acts: &'a [Activity]) -> impl Iterator<Item = &'a Activity> {
    acts.iter().filter(|a| a.modality.is_run())
}

fn line(a: &Activity) -> String {
    format!(
        "{} · {} · {} · {} · {}",
        format::date(day_of(a)),
        a.modality.label(),
        format::opt_km(a.distance()),
        format::opt_pace(a.pace_s_per_km()),
        format::bpm(a.avg_hr)
    )
}

pub fn generate(inp: &Input) -> Vec<Insight> {
    let mut out = Vec::new();
    let by_id: HashMap<i64, &Activity> = inp.acts.iter().map(|a| (a.id, a)).collect();

    // 1. reference tests: HR change between the first and the latest run
    for (name, ids) in &inp.ref_tests {
        let rs: Vec<&Activity> = ids.iter().filter_map(|id| by_id.get(id).copied()).filter(|a| a.avg_hr.is_some()).collect();
        if rs.len() < 2 {
            continue;
        }
        let (first, last) = (rs[0], rs[rs.len() - 1]);
        let diff = last.avg_hr.unwrap() - first.avg_hr.unwrap();
        let pace_diff = match (first.pace_s_per_km(), last.pace_s_per_km()) {
            (Some(a), Some(b)) => b - a,
            _ => 0.0,
        };
        if diff.abs() >= 2.0 {
            let same_pace = pace_diff.abs() <= 10.0;
            let text = if diff < 0.0 {
                if same_pace {
                    trf!(
                        "Sua FC média no teste \"{name}\" caiu {} bpm mantendo o mesmo pace.",
                        "Your average HR in the \"{name}\" test dropped {} bpm at the same pace.",
                        (-diff).round()
                    )
                } else {
                    trf!(
                        "Sua FC média no teste \"{name}\" caiu {} bpm (o pace mudou {} s/km).",
                        "Your average HR in the \"{name}\" test dropped {} bpm (pace changed {} s/km).",
                        (-diff).round(),
                        pace_diff.round()
                    )
                }
            } else {
                trf!(
                    "Sua FC média no teste \"{name}\" subiu {} bpm desde a primeira execução.",
                    "Your average HR in the \"{name}\" test rose {} bpm since the first run.",
                    diff.round()
                )
            };
            out.push(Insight {
                text,
                tone: if diff < 0.0 { Tone::Good } else { Tone::Attention },
                evidence: rs.iter().map(|a| line(a)).collect(),
            });
        }
    }

    // 2. HR at the most common pace: first third vs last third of the runs
    let bands = trends::pace_bands(runs(inp.acts), inp.metrics);
    if let Some(p) = trends::typical_pace(&bands) {
        let h = trends::hr_at_pace(runs(inp.acts), inp.metrics, p);
        if h.len() >= 4 {
            let k = h.len() / 3;
            let k = k.max(1);
            let early: f64 = h[..k].iter().map(|x| x.hr).sum::<f64>() / k as f64;
            let late: f64 = h[h.len() - k..].iter().map(|x| x.hr).sum::<f64>() / k as f64;
            let diff = late - early;
            if diff.abs() >= 2.0 {
                out.push(Insight {
                    text: if diff < 0.0 {
                        trf!(
                            "No pace de {} você está correndo com FC {} bpm menor do que no início.",
                            "At {} pace you are running with a {} bpm lower HR than at the beginning.",
                            format::pace_unit(trends::pace_band(p)),
                            (-diff).round()
                        )
                    } else {
                        trf!(
                            "No pace de {} sua FC está {} bpm maior do que no início.",
                            "At {} pace your HR is {} bpm higher than at the beginning.",
                            format::pace_unit(trends::pace_band(p)),
                            diff.round()
                        )
                    },
                    tone: if diff < 0.0 { Tone::Good } else { Tone::Attention },
                    evidence: h
                        .iter()
                        .map(|x| {
                            format!(
                                "{} · {} · {}{}",
                                format::date(x.date),
                                x.modality.label(),
                                format::bpm(Some(x.hr)),
                                if x.minutes > 0 { trf!(" ({} min estáveis)", " ({} steady min)", x.minutes) } else { String::new() }
                            )
                        })
                        .collect(),
                });
            }
        }
    }

    // 3. pace in the 145–155 bpm band: last 30 days vs before
    let (lo, hi) = (145.0, 155.0);
    let cut = inp.today - Duration::days(30);
    let mut recent = Vec::new();
    let mut before = Vec::new();
    for a in runs(inp.acts) {
        if let Some((pace, n)) = inp.metrics.get(&a.id).and_then(|m| trends::pace_in_hr_band(m, lo, hi)) {
            if day_of(a) > cut { recent.push((a, pace, n)) } else { before.push((a, pace, n)) }
        }
    }
    if !recent.is_empty() && !before.is_empty() {
        let avg = |v: &[(&Activity, f64, usize)]| v.iter().map(|x| x.1 * x.2 as f64).sum::<f64>() / v.iter().map(|x| x.2 as f64).sum::<f64>();
        let (r, b) = (avg(&recent), avg(&before));
        if (r - b).abs() >= 5.0 {
            let mut ev: Vec<String> = before
                .iter()
                .chain(recent.iter())
                .map(|(a, p, n)| format!("{} · {} ({} min)", format::date(day_of(a)), format::pace_unit(*p), n))
                .collect();
            ev.sort();
            out.push(Insight {
                text: if r < b {
                    trf!(
                        "Seu pace médio na faixa de {}–{} bpm melhorou ({} → {}).",
                        "Your average pace in the {}–{} bpm range improved ({} → {}).",
                        lo,
                        hi,
                        format::pace_unit(b),
                        format::pace_unit(r)
                    )
                } else {
                    trf!(
                        "Seu pace médio na faixa de {}–{} bpm ficou mais lento ({} → {}).",
                        "Your average pace in the {}–{} bpm range got slower ({} → {}).",
                        lo,
                        hi,
                        format::pace_unit(b),
                        format::pace_unit(r)
                    )
                },
                tone: if r < b { Tone::Good } else { Tone::Attention },
                evidence: ev,
            });
        }
    }

    // 4. longest run of the last 90 days done in the last 7 days
    let last90: Vec<&Activity> = runs(inp.acts).filter(|a| day_of(a) > inp.today - Duration::days(90)).collect();
    if last90.len() >= 3 {
        if let Some(longest) = last90.iter().max_by(|a, b| a.distance().unwrap_or(0.0).total_cmp(&b.distance().unwrap_or(0.0))) {
            if day_of(longest) > inp.today - Duration::days(7) && longest.distance().unwrap_or(0.0) > 0.0 {
                out.push(Insight {
                    text: trf!(
                        "Você atingiu seu maior longão dos últimos 90 dias: {}.",
                        "You did your longest run of the last 90 days: {}.",
                        format::opt_km(longest.distance())
                    ),
                    tone: Tone::Good,
                    evidence: vec![line(longest), trf!("{} corridas nos últimos 90 dias", "{} runs in the last 90 days", last90.len())],
                });
            }
        }
    }

    // 5. runs per week in the last 4 weeks
    let start4 = inp.today - Duration::days(27);
    let n4: Vec<&Activity> = runs(inp.acts).filter(|a| day_of(a) >= start4 && day_of(a) <= inp.today).collect();
    if !n4.is_empty() {
        out.push(Insight {
            text: trf!(
                "Nas últimas quatro semanas você correu em média {} vezes por semana.",
                "In the last four weeks you ran on average {} times a week.",
                format::num(n4.len() as f64 / 4.0, 1)
            ),
            tone: Tone::Neutral,
            evidence: n4.iter().map(|a| line(a)).collect(),
        });
    }

    // 6. runs on the day after tennis: HR compared with other runs at a similar pace
    let tennis_days: std::collections::HashSet<NaiveDate> =
        inp.acts.iter().filter(|a| a.modality == Modality::Tennis).map(day_of).collect();
    if !tennis_days.is_empty() {
        let (after, other): (Vec<&Activity>, Vec<&Activity>) = runs(inp.acts)
            .filter(|a| a.avg_hr.is_some() && a.pace_s_per_km().is_some())
            .partition(|a| tennis_days.contains(&(day_of(a) - Duration::days(1))));
        if after.len() >= 2 && other.len() >= 3 {
            // HR per (m/min): compares effort independent of pace
            let eff = |v: &[&Activity]| {
                v.iter().map(|a| a.avg_hr.unwrap() / (60_000.0 / a.pace_s_per_km().unwrap())).sum::<f64>() / v.len() as f64
            };
            let (ea, eo) = (eff(&after), eff(&other));
            let pct = (ea - eo) / eo * 100.0;
            if pct.abs() >= 2.0 {
                let mut ev: Vec<String> = after.iter().map(|a| format!("{} {}", tr!("[após tênis]", "[after tennis]"), line(a))).collect();
                ev.extend(other.iter().map(|a| line(a)));
                out.push(Insight {
                    text: if pct > 0.0 {
                        trf!(
                            "Corridas feitas no dia seguinte ao tênis apresentam FC média maior ({}% mais batimentos por metro).",
                            "Runs on the day after tennis show a higher average HR ({}% more beats per meter).",
                            format::num(pct, 0)
                        )
                    } else {
                        trf!(
                            "Corridas no dia seguinte ao tênis não ficaram mais pesadas ({}% de batimentos por metro).",
                            "Runs on the day after tennis were not harder ({}% beats per meter).",
                            format::num(pct, 0)
                        )
                    },
                    tone: if pct > 0.0 { Tone::Attention } else { Tone::Neutral },
                    evidence: ev,
                });
            }
        }
    }

    // 7. running volume of the last 7 days vs the 4 weeks before
    let km = |from: NaiveDate, to: NaiveDate| -> f64 {
        runs(inp.acts).filter(|a| (from..=to).contains(&day_of(a))).filter_map(|a| a.distance()).sum::<f64>() / 1000.0
    };
    let last7 = km(inp.today - Duration::days(6), inp.today);
    let prev28 = km(inp.today - Duration::days(34), inp.today - Duration::days(7));
    let weeks_with_runs = (1..=4)
        .filter(|k| {
            let end = inp.today - Duration::days(7 * k);
            km(end - Duration::days(6), end) > 0.0
        })
        .count();
    if weeks_with_runs >= 2 && prev28 > 0.0 {
        let avg = prev28 / 4.0;
        let pct = (last7 - avg) / avg * 100.0;
        if pct.abs() >= 15.0 {
            out.push(Insight {
                text: if pct > 0.0 {
                    trf!(
                        "Seu volume de corrida aumentou {}% em relação à média recente.",
                        "Your running volume rose {}% compared with the recent average.",
                        format::num(pct, 0)
                    )
                } else {
                    trf!(
                        "Seu volume de corrida caiu {}% em relação à média recente.",
                        "Your running volume dropped {}% compared with the recent average.",
                        format::num(-pct, 0)
                    )
                },
                tone: if pct > 30.0 { Tone::Attention } else { Tone::Neutral },
                evidence: vec![
                    trf!("últimos 7 dias: {} km", "last 7 days: {} km", format::num(last7, 1)),
                    trf!("média semanal das 4 semanas anteriores: {} km", "weekly average of the previous 4 weeks: {} km", format::num(avg, 1)),
                ],
            });
        }
    }

    // 8. cardiac drift: last 30 days vs before
    let drift = |a: &Activity| inp.metrics.get(&a.id).and_then(|m| m.drift.as_ref()).and_then(|d| d.decoupling_pct);
    let recent_d: Vec<(&Activity, f64)> = runs(inp.acts).filter(|a| day_of(a) > cut).filter_map(|a| drift(a).map(|d| (a, d))).collect();
    let before_d: Vec<(&Activity, f64)> = runs(inp.acts).filter(|a| day_of(a) <= cut).filter_map(|a| drift(a).map(|d| (a, d))).collect();
    if recent_d.len() >= 2 && before_d.len() >= 2 {
        let avg = |v: &[(&Activity, f64)]| v.iter().map(|x| x.1).sum::<f64>() / v.len() as f64;
        let (r, b) = (avg(&recent_d), avg(&before_d));
        if (r - b).abs() >= 1.5 {
            out.push(Insight {
                text: if r < b {
                    trf!(
                        "Sua deriva cardíaca média caiu de {}% para {}%: sinal de melhor eficiência aeróbica.",
                        "Your average cardiac drift dropped from {}% to {}%: a sign of better aerobic efficiency.",
                        format::num(b, 1),
                        format::num(r, 1)
                    )
                } else {
                    trf!(
                        "Sua deriva cardíaca média subiu de {}% para {}%.",
                        "Your average cardiac drift rose from {}% to {}%.",
                        format::num(b, 1),
                        format::num(r, 1)
                    )
                },
                tone: if r < b { Tone::Good } else { Tone::Attention },
                evidence: before_d
                    .iter()
                    .chain(recent_d.iter())
                    .map(|(a, d)| format!("{} · {}%", line(a), format::num(*d, 1)))
                    .collect(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: i64, date: NaiveDate, km: f64, pace: f64, hr: f64) -> Activity {
        Activity {
            id,
            start_time: date.and_hms_opt(7, 0, 0).unwrap().and_utc().timestamp(),
            modality: Modality::RoadRun,
            distance_m: Some(km * 1000.0),
            moving_s: Some(km * pace),
            avg_hr: Some(hr),
            ..Default::default()
        }
    }

    #[test]
    fn reference_test_insight_with_evidence() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        let acts = vec![
            run(1, NaiveDate::from_ymd_opt(2026, 5, 3).unwrap(), 5.0, 362.0, 179.0),
            run(2, NaiveDate::from_ymd_opt(2026, 8, 26).unwrap(), 5.0, 358.0, 168.0),
        ];
        let inp = Input { acts: &acts, metrics: &HashMap::new(), ref_tests: vec![("5 km".into(), vec![1, 2])], today };
        let ins = generate(&inp);
        let t = ins.iter().find(|i| i.text.contains("caiu 11 bpm")).expect("insight");
        assert_eq!(t.tone, Tone::Good);
        assert_eq!(t.evidence.len(), 2);
    }

    #[test]
    fn runs_per_week_and_volume() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        let mut acts = Vec::new();
        for (i, back) in [1, 3, 9, 12, 16, 20, 24, 26, 30, 33].iter().enumerate() {
            acts.push(run(i as i64, today - Duration::days(*back), 5.0, 360.0, 160.0));
        }
        acts.push(run(99, today, 12.0, 380.0, 150.0));
        let inp = Input { acts: &acts, metrics: &HashMap::new(), ref_tests: vec![], today };
        let ins = generate(&inp);
        assert!(ins.iter().any(|i| i.text.contains("vezes por semana")));
        assert!(ins.iter().any(|i| i.text.contains("aumentou")), "{:?}", ins.iter().map(|i| &i.text).collect::<Vec<_>>());
        assert!(ins.iter().any(|i| i.text.contains("maior longão")));
    }
}
