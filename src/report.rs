//! Reports and exports: a self-contained HTML report (opens in any browser,
//! prints to PDF) and CSV/JSON tables of runs, the plan and weekly totals.

use crate::analysis::metrics::Metrics;
use crate::coach::fitness::day;
use crate::coach::store::CoachState;
use crate::coach::{Status, week_start};
use crate::format;
use crate::model::Activity;
use chrono::{Duration, NaiveDate};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Weeks4,
    Weeks12,
    SinceGoal,
    All,
}

impl Period {
    pub const ALL: [Period; 4] = [Period::Weeks4, Period::Weeks12, Period::SinceGoal, Period::All];

    pub fn label(self) -> &'static str {
        match self {
            Period::Weeks4 => tr!("Últimas 4 semanas", "Last 4 weeks"),
            Period::Weeks12 => tr!("Últimas 12 semanas", "Last 12 weeks"),
            Period::SinceGoal => tr!("Desde o início da meta", "Since the goal started"),
            Period::All => tr!("Todo o histórico", "All history"),
        }
    }

    pub fn from(self, today: NaiveDate, coach: Option<&CoachState>, first: Option<NaiveDate>) -> NaiveDate {
        match self {
            Period::Weeks4 => today - Duration::days(27),
            Period::Weeks12 => today - Duration::days(83),
            Period::SinceGoal => coach.map(|c| c.goal.start_date).or(first).unwrap_or(today),
            Period::All => first.unwrap_or(today),
        }
    }
}

/// A table: header + rows (numbers stay numbers in JSON).
pub struct Table {
    pub header: Vec<&'static str>,
    pub rows: Vec<Vec<Value>>,
}

fn round(v: Option<f64>, d: i32) -> Value {
    match v {
        Some(x) if x.is_finite() => {
            let k = 10f64.powi(d);
            json!((x * k).round() / k)
        }
        _ => Value::Null,
    }
}

pub fn runs_table(runs: &[&Activity], metrics: &HashMap<i64, Metrics>, coach: Option<&CoachState>) -> Table {
    let planned: HashMap<i64, &crate::coach::adapt::Tracked> = coach
        .map(|c| {
            c.sessions
                .iter()
                .filter_map(|s| s.activity_id.map(|a| (a, s)))
                .collect()
        })
        .unwrap_or_default();
    let rows = runs
        .iter()
        .map(|a| {
            let m = metrics.get(&a.id);
            let p = planned.get(&a.id);
            vec![
                json!(day(a).format("%Y-%m-%d").to_string()),
                json!(a.local_start().format("%H:%M").to_string()),
                json!(a.modality.code()),
                round(a.distance().map(|d| d / 1000.0), 3),
                round(a.duration(), 0),
                a.pace_s_per_km().map(|p| json!(format::pace(p))).unwrap_or(Value::Null),
                round(a.avg_hr, 0),
                round(a.max_hr, 0),
                round(a.avg_cadence, 0),
                round(a.ascent_m.or(m.and_then(|m| m.gain_m)), 0),
                round(a.incline_pct, 1),
                round(m.and_then(|m| m.drift.as_ref()).and_then(|d| d.decoupling_pct), 1),
                round(m.map(|m| m.load), 0),
                p.map(|t| json!(t.session.kind.label())).unwrap_or(Value::Null),
                p.map(|t| json!(t.status.label())).unwrap_or(Value::Null),
                json!(a.rpe),
                json!(a.notes.clone().unwrap_or_default()),
            ]
        })
        .collect();
    Table {
        header: vec![
            "date",
            "time",
            "type",
            "distance_km",
            "duration_s",
            "pace_min_km",
            "avg_hr",
            "max_hr",
            "cadence_spm",
            "ascent_m",
            "incline_pct",
            "decoupling_pct",
            "load",
            "planned_session",
            "plan_status",
            "rpe",
            "notes",
        ],
        rows,
    }
}

pub fn plan_table(coach: &CoachState) -> Table {
    let rows = coach
        .sessions
        .iter()
        .map(|t| {
            let s = &t.session;
            vec![
                json!(s.date.format("%Y-%m-%d").to_string()),
                json!(s.week),
                json!(s.phase.label()),
                json!(s.kind.label()),
                round(Some(s.duration_s / 60.0), 0),
                round(Some(s.distance_m / 1000.0), 2),
                s.pace
                    .map(|p| json!(format!("{}-{}", format::pace(p.0), format::pace(p.1))))
                    .unwrap_or(Value::Null),
                s.pace
                    .map(|p| {
                        json!(format!(
                            "{}-{}",
                            format::num(format::pace_to_kmh(p.1), 1),
                            format::num(format::pace_to_kmh(p.0), 1)
                        ))
                    })
                    .unwrap_or(Value::Null),
                round(s.hr_max, 0),
                json!(if s.treadmill { "treadmill" } else { "road" }),
                json!(t.status.code()),
                json!(t.activity_id),
            ]
        })
        .collect();
    Table {
        header: vec![
            "date",
            "week",
            "phase",
            "session",
            "minutes",
            "distance_km",
            "pace",
            "treadmill_kmh",
            "hr_max",
            "surface",
            "status",
            "activity_id",
        ],
        rows,
    }
}

#[derive(Default, Clone)]
pub struct Week {
    pub runs: usize,
    pub km: f64,
    pub secs: f64,
    pub hr_sum: f64,
    pub hr_w: f64,
    pub longest_km: f64,
    pub planned: usize,
    pub done: usize,
}

pub fn weeks(
    runs: &[&Activity],
    coach: Option<&CoachState>,
    from: NaiveDate,
    to: NaiveDate,
) -> BTreeMap<NaiveDate, Week> {
    let mut out: BTreeMap<NaiveDate, Week> = BTreeMap::new();
    let mut d = week_start(from);
    while d <= to {
        out.insert(d, Week::default());
        d += Duration::days(7);
    }
    for a in runs {
        let w = out.entry(week_start(day(a))).or_default();
        let km = a.distance().unwrap_or(0.0) / 1000.0;
        let secs = a.duration().unwrap_or(0.0);
        w.runs += 1;
        w.km += km;
        w.secs += secs;
        w.longest_km = w.longest_km.max(km);
        if let Some(h) = a.avg_hr {
            w.hr_sum += h * secs;
            w.hr_w += secs;
        }
    }
    if let Some(c) = coach {
        for t in c
            .sessions
            .iter()
            .filter(|t| t.session.date >= from && t.session.date <= to && t.status != Status::Planned)
        {
            let w = out.entry(week_start(t.session.date)).or_default();
            w.planned += 1;
            if matches!(t.status, Status::Done | Status::Partial) {
                w.done += 1;
            }
        }
    }
    out
}

pub fn weeks_table(w: &BTreeMap<NaiveDate, Week>) -> Table {
    Table {
        header: vec![
            "week",
            "runs",
            "km",
            "hours",
            "avg_hr",
            "longest_km",
            "planned_sessions",
            "done_sessions",
        ],
        rows: w
            .iter()
            .map(|(d, x)| {
                vec![
                    json!(d.format("%Y-%m-%d").to_string()),
                    json!(x.runs),
                    round(Some(x.km), 2),
                    round(Some(x.secs / 3600.0), 2),
                    round((x.hr_w > 0.0).then(|| x.hr_sum / x.hr_w), 0),
                    round(Some(x.longest_km), 2),
                    json!(x.planned),
                    json!(x.done),
                ]
            })
            .collect(),
    }
}

fn csv_cell(v: &Value) -> String {
    let s = match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if s.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

/// CSV with comma separator and UTF-8 BOM (opens correctly in Excel).
pub fn to_csv(t: &Table) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str(&t.header.join(","));
    out.push_str("\r\n");
    for r in &t.rows {
        out.push_str(&r.iter().map(csv_cell).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

pub fn to_json(t: &Table) -> String {
    let rows: Vec<Value> = t
        .rows
        .iter()
        .map(|r| {
            Value::Object(
                t.header
                    .iter()
                    .zip(r.iter())
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
            )
        })
        .collect();
    serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".into())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Small inline SVG line chart (x = date, y = value). `invert`: lower is better on top.
fn svg_line(
    points: &[(NaiveDate, f64)],
    color: &str,
    invert: bool,
    fmt: &dyn Fn(f64) -> String,
    target: Option<f64>,
) -> String {
    if points.len() < 2 {
        return format!(
            "<p class=muted>{}</p>",
            esc(tr!(
                "Dados insuficientes para o gráfico.",
                "Not enough data for the chart."
            ))
        );
    }
    let (w, h, pad) = (640.0, 200.0, 36.0);
    let x0 = points.first().unwrap().0;
    let span = (points.last().unwrap().0 - x0).num_days().max(1) as f64;
    let mut lo = points.iter().map(|p| p.1).fold(f64::MAX, f64::min);
    let mut hi = points.iter().map(|p| p.1).fold(f64::MIN, f64::max);
    if let Some(t) = target {
        lo = lo.min(t);
        hi = hi.max(t);
    }
    let margin = ((hi - lo) * 0.1).max(1.0);
    lo -= margin;
    hi += margin;
    let y = |v: f64| {
        let f = (v - lo) / (hi - lo);
        if invert {
            pad + f * (h - 2.0 * pad)
        } else {
            h - pad - f * (h - 2.0 * pad)
        }
    };
    let x = |d: NaiveDate| pad + (d - x0).num_days() as f64 / span * (w - 2.0 * pad);
    let mut s = format!("<svg viewBox='0 0 {w} {h}' class=chart role=img>");
    for k in 0..=4 {
        let v = lo + (hi - lo) * k as f64 / 4.0;
        let _ = write!(
            s,
            "<line x1={pad} x2={} y1={y1:.1} y2={y1:.1} class=grid /><text x=4 y={:.1} class=axis>{}</text>",
            w - pad,
            y(v) + 4.0,
            esc(&fmt(v)),
            y1 = y(v)
        );
    }
    if let Some(t) = target {
        let _ = write!(
            s,
            "<line x1={pad} x2={} y1={yt:.1} y2={yt:.1} class=target /><text x={} y={:.1} class=axis>{}</text>",
            w - pad,
            w - pad - 60.0,
            y(t) - 4.0,
            esc(tr!("meta", "goal")),
            yt = y(t)
        );
    }
    let path: Vec<String> = points
        .iter()
        .map(|(d, v)| format!("{:.1},{:.1}", x(*d), y(*v)))
        .collect();
    let _ = write!(
        s,
        "<polyline fill=none stroke='{color}' stroke-width=2 points='{}' />",
        path.join(" ")
    );
    for (d, v) in points {
        let _ = write!(
            s,
            "<circle cx={:.1} cy={:.1} r=3 fill='{color}'><title>{} · {}</title></circle>",
            x(*d),
            y(*v),
            format::date(*d),
            esc(&fmt(*v))
        );
    }
    let _ = write!(
        s,
        "<text x={pad} y={} class=axis>{}</text><text x={} y={} class=axis text-anchor=end>{}</text></svg>",
        h - 6.0,
        format::date(x0),
        w - pad,
        h - 6.0,
        format::date(points.last().unwrap().0)
    );
    s
}

fn table_html(t: &Table, headers: &[&str]) -> String {
    let mut s = String::from("<table><thead><tr>");
    for h in headers {
        let _ = write!(s, "<th>{}</th>", esc(h));
    }
    s.push_str("</tr></thead><tbody>");
    for r in &t.rows {
        s.push_str("<tr>");
        for v in r {
            let _ = write!(
                s,
                "<td>{}</td>",
                esc(&csv_cell(v).trim_matches('"').replace("\"\"", "\""))
            );
        }
        s.push_str("</tr>");
    }
    s.push_str("</tbody></table>");
    s
}

/// Self-contained HTML report for the period.
pub fn html(
    runs_all: &[Activity],
    metrics: &HashMap<i64, Metrics>,
    coach: Option<&CoachState>,
    from: NaiveDate,
    to: NaiveDate,
) -> String {
    let runs: Vec<&Activity> = runs_all.iter().filter(|a| day(a) >= from && day(a) <= to).collect();
    let total_km: f64 = runs.iter().filter_map(|a| a.distance()).sum::<f64>() / 1000.0;
    let total_s: f64 = runs.iter().filter_map(|a| a.duration()).sum();
    let longest = runs.iter().filter_map(|a| a.distance()).fold(0.0, f64::max);
    let mut h = String::new();
    let _ = write!(
        h,
        "<!doctype html><html lang='{}'><head><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>FitLedger · {}</title><style>{}</style></head><body><main>",
        if crate::i18n::pt() { "pt-BR" } else { "en" },
        esc(tr!("Relatório", "Report")),
        CSS
    );
    let _ = write!(
        h,
        "<header><h1>FitLedger</h1><p class=muted>{} · {} – {}</p></header>",
        esc(tr!("Relatório de corrida", "Running report")),
        format::date(from),
        format::date(to)
    );
    // summary
    let _ = write!(
        h,
        "<section class=cards><div><b>{}</b><span>{}</span></div><div><b>{} km</b><span>{}</span></div><div><b>{}</b><span>{}</span></div><div><b>{}</b><span>{}</span></div></section>",
        runs.len(),
        esc(tr!("corridas", "runs")),
        format::num(total_km, 1),
        esc(tr!("distância", "distance")),
        format::hours_short(total_s),
        esc(tr!("tempo correndo", "time running")),
        format::km(longest),
        esc(tr!("maior corrida", "longest run"))
    );
    // goal
    if let Some(c) = coach {
        let _ = write!(
            h,
            "<h2>{}</h2><p><b>{}</b></p>",
            esc(tr!("Meta", "Goal")),
            esc(&c.goal.describe())
        );
        match c.reached {
            Some((d, _)) => {
                let _ = write!(
                    h,
                    "<p class=good>{}</p>",
                    esc(&trf!("Meta atingida em {}.", "Goal reached on {}.", format::date(d)))
                );
            }
            None => {
                let _ = write!(
                    h,
                    "<p>{}</p>",
                    esc(&trf!(
                        "Estimativa: {} ({} semanas).",
                        "Estimate: {} ({} weeks).",
                        format::date(c.projection.date),
                        c.projection.weeks
                    ))
                );
            }
        }
        h.push_str("<table><thead><tr>");
        for t in [tr!("Item", "Item"), tr!("Agora", "Now"), tr!("Meta", "Goal"), "%"] {
            let _ = write!(h, "<th>{}</th>", esc(t));
        }
        h.push_str("</tr></thead><tbody>");
        for p in &c.projection.parts {
            let _ = write!(
                h,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}%</td></tr>",
                esc(&p.label),
                esc(&p.now),
                esc(&p.goal),
                (p.fraction * 100.0).round()
            );
        }
        h.push_str("</tbody></table>");
        let hist: Vec<(NaiveDate, f64)> = c
            .fitness
            .hr_history
            .iter()
            .filter(|(d, _)| *d >= from && *d <= to)
            .cloned()
            .collect();
        if !hist.is_empty() {
            let pace = c.goal.goal_pace().unwrap_or(c.fitness.easy_pace);
            let _ = write!(
                h,
                "<h3>{}</h3>",
                esc(&trf!(
                    "FC estimada no pace {}",
                    "Estimated HR at {} pace",
                    format::pace_unit(pace)
                ))
            );
            h.push_str(&svg_line(
                &hist,
                "#ef4444",
                false,
                &|v| format!("{} bpm", v.round()),
                c.goal.target_hr,
            ));
        }
        let notes: Vec<String> = c.adjust.notes.iter().map(|n| n.text()).collect();
        if !notes.is_empty() {
            let _ = write!(
                h,
                "<h3>{}</h3><ul>",
                esc(tr!("Observações do treinador", "Coach notes"))
            );
            for n in notes {
                let _ = write!(h, "<li>{}</li>", esc(&n));
            }
            h.push_str("</ul>");
        }
    }
    // weekly volume and adherence
    let w = weeks(&runs, coach, from, to);
    let km: Vec<(NaiveDate, f64)> = w.iter().map(|(d, x)| (*d, x.km)).collect();
    let _ = write!(h, "<h2>{}</h2>", esc(tr!("Volume semanal", "Weekly volume")));
    h.push_str(&svg_line(
        &km,
        "#3b82f6",
        false,
        &|v| format!("{} km", format::num(v, 1)),
        None,
    ));
    let planned: usize = w.values().map(|x| x.planned).sum();
    let done: usize = w.values().map(|x| x.done).sum();
    if planned > 0 {
        let _ = write!(
            h,
            "<p>{}</p>",
            esc(&trf!(
                "Aderência ao plano: {done} de {planned} treinos ({}%).",
                "Plan adherence: {done} of {planned} sessions ({}%).",
                (done as f64 / planned as f64 * 100.0).round()
            ))
        );
    }
    h.push_str(&table_html(
        &weeks_table(&w),
        &[
            tr!("Semana", "Week"),
            tr!("Corridas", "Runs"),
            "km",
            tr!("Horas", "Hours"),
            tr!("FC média", "Avg HR"),
            tr!("Maior (km)", "Longest (km)"),
            tr!("Planejados", "Planned"),
            tr!("Feitos", "Done"),
        ],
    ));
    // pace and drift over time
    let pace: Vec<(NaiveDate, f64)> = runs.iter().filter_map(|a| Some((day(a), a.pace_s_per_km()?))).collect();
    let _ = write!(
        h,
        "<h2>{}</h2>",
        esc(tr!("Pace médio por corrida", "Average pace per run"))
    );
    h.push_str(&svg_line(&pace, "#06b6d4", true, &format::pace, None));
    let drift: Vec<(NaiveDate, f64)> = runs
        .iter()
        .filter_map(|a| Some((day(a), metrics.get(&a.id)?.drift.as_ref()?.decoupling_pct?)))
        .collect();
    if drift.len() >= 2 {
        let _ = write!(
            h,
            "<h2>{}</h2>",
            esc(tr!("Deriva cardíaca (desacoplamento)", "Cardiac drift (decoupling)"))
        );
        h.push_str(&svg_line(
            &drift,
            "#f59e0b",
            false,
            &|v| format!("{}%", format::num(v, 1)),
            Some(5.0),
        ));
    }
    // runs
    let _ = write!(h, "<h2>{}</h2>", esc(tr!("Corridas", "Runs")));
    let mut sorted = runs.clone();
    sorted.sort_by_key(|a| std::cmp::Reverse(a.start_time));
    h.push_str(&table_html(
        &runs_table(&sorted, metrics, coach),
        &[
            tr!("Data", "Date"),
            tr!("Hora", "Time"),
            tr!("Tipo", "Type"),
            "km",
            tr!("Duração (s)", "Duration (s)"),
            "Pace",
            tr!("FC méd", "Avg HR"),
            tr!("FC máx", "Max HR"),
            tr!("Cadência", "Cadence"),
            tr!("Subida (m)", "Ascent (m)"),
            tr!("Inclinação %", "Incline %"),
            tr!("Deriva %", "Drift %"),
            tr!("Carga", "Load"),
            tr!("Sessão do plano", "Plan session"),
            tr!("Status", "Status"),
            "RPE",
            tr!("Notas", "Notes"),
        ],
    ));
    let _ = write!(
        h,
        "<footer class=muted>{}</footer></main></body></html>",
        esc(tr!(
            "Gerado pelo FitLedger. Orientação de treino, não é avaliação médica.",
            "Generated by FitLedger. Training guidance, not medical advice."
        ))
    );
    h
}

const CSS: &str = "
:root{--bg:#fff;--fg:#111827;--muted:#6b7280;--line:#e5e7eb;--card:#f9fafb}
@media (prefers-color-scheme:dark){:root{--bg:#0f1115;--fg:#e5e7eb;--muted:#9ca3af;--line:#262a33;--card:#171a21}}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,Segoe UI,sans-serif}
main{max-width:960px;margin:0 auto;padding:24px 16px}
h1{margin:0;color:#3b82f6}h2{margin-top:32px;border-bottom:1px solid var(--line);padding-bottom:4px}
.muted{color:var(--muted)}.good{color:#16a34a;font-weight:600}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin-top:16px}
.cards div{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:12px}
.cards b{display:block;font-size:22px}.cards span{color:var(--muted);font-size:13px}
table{border-collapse:collapse;width:100%;font-size:13px;margin:8px 0;display:block;overflow-x:auto}
th,td{border-bottom:1px solid var(--line);padding:4px 8px;text-align:left;white-space:nowrap}
.chart{width:100%;height:auto;background:var(--card);border:1px solid var(--line);border-radius:10px}
.grid{stroke:var(--line)}.target{stroke:#16a34a;stroke-dasharray:6 4}
.axis{fill:var(--muted);font-size:11px}
footer{margin-top:40px;font-size:12px}
@media print{.chart{break-inside:avoid}}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_and_json() {
        let t = Table {
            header: vec!["a", "b", "c"],
            rows: vec![vec![json!(1), json!("x, \"y\""), Value::Null]],
        };
        let csv = to_csv(&t);
        assert!(csv.contains("1,\"x, \"\"y\"\"\",\r\n"));
        let j: Vec<Value> = serde_json::from_str(&to_json(&t)).unwrap();
        assert_eq!(j[0]["a"], json!(1));
    }

    #[test]
    fn html_report_without_and_with_data() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let empty = html(&[], &HashMap::new(), None, today - Duration::days(27), today);
        assert!(empty.starts_with("<!doctype html>"));
        let runs: Vec<Activity> = (0..6)
            .map(|i| Activity {
                id: i,
                start_time: (today - Duration::days(i * 3))
                    .and_hms_opt(7, 0, 0)
                    .unwrap()
                    .and_utc()
                    .timestamp(),
                modality: crate::model::Modality::Treadmill,
                distance_m: Some(5000.0),
                moving_s: Some(1800.0 + i as f64 * 10.0),
                avg_hr: Some(160.0 + i as f64),
                notes: Some("<b>quente</b>".into()),
                ..Default::default()
            })
            .collect();
        let r = html(&runs, &HashMap::new(), None, today - Duration::days(27), today);
        assert!(r.contains("<svg"));
        assert!(r.contains("&lt;b&gt;quente"), "notes are escaped");
        let n_weeks = weeks(
            &runs.iter().collect::<Vec<_>>(),
            None,
            today - Duration::days(27),
            today,
        )
        .len();
        // two header rows + one row per week + one row per run
        assert_eq!(r.matches("<tr>").count(), 2 + n_weeks + 6);
    }
}
