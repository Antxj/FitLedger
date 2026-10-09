//! Chart helpers on top of egui_plot: hover shows the values in the right
//! unit (bpm, pace, km/h...), axes are formatted, and charts of the same
//! activity share the cursor and the x axis.

use crate::format;
use eframe::egui::{self, Color32, Ui};
use egui_plot::{Bar, BarChart, GridMark, HoverPosition, Legend, Line, Plot, PlotPoints, Points};
use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Axis {
    /// Seconds → "mm:ss" / "h:mm:ss".
    Time,
    /// Meters → km.
    Distance,
    /// Days since 1970-01-01 → date.
    Date,
    Bpm,
    /// Seconds per km → "m:ss".
    Pace,
    /// m/s shown as km/h.
    SpeedKmh,
    Cadence,
    Meters,
    Watts,
    Percent,
    Hours,
    Plain,
}

impl Axis {
    pub fn fmt(self, v: f64) -> String {
        match self {
            Axis::Time => format::duration(v.max(0.0)),
            Axis::Distance => format!("{} km", format::num(v / 1000.0, 2)),
            Axis::Date => chrono::NaiveDate::from_num_days_from_ce_opt(v.round() as i32 + 719_163)
                .map(format::date_short)
                .unwrap_or_default(),
            Axis::Bpm => format!("{} bpm", v.round()),
            Axis::Pace => format::pace_unit(v),
            Axis::SpeedKmh => format!("{} km/h", format::num(v, 1)),
            Axis::Cadence => format!("{} spm", v.round()),
            Axis::Meters => format!("{} m", v.round()),
            Axis::Watts => format!("{} W", v.round()),
            Axis::Percent => format!("{}%", format::num(v, 1)),
            Axis::Hours => format::hours_short(v * 3600.0),
            Axis::Plain => format::num_trim(v, 1),
        }
    }

    fn tick(self, v: f64) -> String {
        match self {
            Axis::Distance => format::num_trim(v / 1000.0, 1),
            Axis::Bpm | Axis::Cadence | Axis::Meters | Axis::Watts => format!("{}", v.round()),
            Axis::SpeedKmh => format::num_trim(v, 1),
            Axis::Pace => format::pace(v),
            Axis::Percent => format!("{}%", format::num_trim(v, 0)),
            _ => self.fmt(v),
        }
    }
}

pub fn date_to_x(d: chrono::NaiveDate) -> f64 {
    (d.num_days_from_ce() - 719_163) as f64
}

use chrono::Datelike;

pub struct Series {
    pub name: String,
    pub color: Color32,
    pub points: Vec<[f64; 2]>,
    /// Draw as dots instead of a line.
    pub dots: bool,
}

impl Series {
    pub fn line(name: impl Into<String>, color: Color32, points: Vec<[f64; 2]>) -> Self {
        Series { name: name.into(), color, points, dots: false }
    }

    pub fn dots(name: impl Into<String>, color: Color32, points: Vec<[f64; 2]>) -> Self {
        Series { name: name.into(), color, points, dots: true }
    }
}

pub struct ChartOpts<'a> {
    pub id: &'a str,
    pub height: f32,
    pub x: Axis,
    pub y: Axis,
    /// Charts with the same group share cursor and x axis.
    pub link: Option<&'a str>,
    pub invert_y: bool,
    pub legend: bool,
    pub title: Option<&'a str>,
    /// Horizontal reference lines (value, label).
    pub hlines: Vec<(f64, String)>,
}

impl<'a> ChartOpts<'a> {
    pub fn new(id: &'a str, x: Axis, y: Axis) -> Self {
        ChartOpts { id, height: 180.0, x, y, link: None, invert_y: false, legend: false, title: None, hlines: Vec::new() }
    }
}

/// Keeps at most `max` points (min/max per bucket so spikes stay visible).
pub fn downsample(points: &[[f64; 2]], max: usize) -> Vec<[f64; 2]> {
    if points.len() <= max || max < 4 {
        return points.to_vec();
    }
    let bucket = points.len().div_ceil(max / 2);
    let mut out = Vec::with_capacity(max + 2);
    for chunk in points.chunks(bucket) {
        let (mut lo, mut hi) = (chunk[0], chunk[0]);
        for p in chunk {
            if p[1] < lo[1] {
                lo = *p;
            }
            if p[1] > hi[1] {
                hi = *p;
            }
        }
        if lo[0] <= hi[0] {
            out.push(lo);
            if hi != lo {
                out.push(hi);
            }
        } else {
            out.push(hi);
            out.push(lo);
        }
    }
    out
}

pub fn line_chart(ui: &mut Ui, opts: ChartOpts<'_>, series: Vec<Series>) {
    if let Some(t) = opts.title {
        ui.label(egui::RichText::new(t).strong());
    }
    let (xa, ya) = (opts.x, opts.y);
    let mut plot = Plot::new(egui::Id::new(opts.id))
        .height(opts.height)
        .allow_scroll(false)
        .allow_boxed_zoom(true)
        .allow_drag(egui::Vec2b::new(true, false))
        .allow_zoom(egui::Vec2b::new(true, false))
        .invert_y(opts.invert_y)
        .x_axis_formatter(move |m: GridMark, _r: &RangeInclusive<f64>| xa.tick(m.value))
        .y_axis_formatter(move |m: GridMark, _r: &RangeInclusive<f64>| ya.tick(m.value))
        .y_axis_min_width(40.0)
        .label_formatter(move |pos: &HoverPosition<'_>| match pos {
            HoverPosition::NearDataPoint { plot_name, position, .. } => {
                let name = if plot_name.is_empty() { String::new() } else { format!("{plot_name}\n") };
                Some(format!("{name}{}\n{}", ya.fmt(position.y), xa.fmt(position.x)))
            }
            HoverPosition::Elsewhere { position } => Some(xa.fmt(position.x)),
        });
    if let Some(group) = opts.link {
        let group = egui::Id::new(group);
        plot = plot.link_cursor(group, egui::Vec2b::new(true, false)).link_axis(group, egui::Vec2b::new(true, false));
    }
    if opts.legend {
        plot = plot.legend(Legend::default());
    }
    let hlines = opts.hlines;
    plot.show(ui, |p| {
        for s in series {
            let pts = downsample(&s.points, 2500);
            if s.dots {
                p.points(Points::new(s.name, PlotPoints::new(pts)).color(s.color).radius(3.5).filled(true));
            } else {
                p.line(Line::new(s.name, PlotPoints::new(pts)).color(s.color).width(1.6));
            }
        }
        for (v, label) in hlines {
            p.hline(egui_plot::HLine::new(label, v).color(Color32::GRAY).style(egui_plot::LineStyle::dashed_loose()));
        }
    });
}

/// Bars: (x, height, color, tooltip text).
pub fn bar_chart(ui: &mut Ui, opts: ChartOpts<'_>, bars: Vec<(f64, f64, Color32, String)>, bar_width: f64) {
    if let Some(t) = opts.title {
        ui.label(egui::RichText::new(t).strong());
    }
    let (xa, ya) = (opts.x, opts.y);
    let items: Vec<Bar> = bars
        .into_iter()
        .map(|(x, h, c, name)| Bar::new(x, h).fill(c).width(bar_width).name(name))
        .collect();
    let chart = BarChart::new("", items).element_formatter(Box::new(move |b: &Bar, _c: &BarChart| {
        let label = if b.name.is_empty() { xa.fmt(b.argument) } else { b.name.clone() };
        format!("{label}\n{}", ya.fmt(b.value))
    }));
    let mut plot = Plot::new(egui::Id::new(opts.id))
        .height(opts.height)
        .allow_scroll(false)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_boxed_zoom(false)
        .x_axis_formatter(move |m: GridMark, _r: &RangeInclusive<f64>| xa.tick(m.value))
        .y_axis_formatter(move |m: GridMark, _r: &RangeInclusive<f64>| ya.tick(m.value))
        .y_axis_min_width(40.0)
        .include_y(0.0)
        .label_formatter(|_| None);
    if let Some(group) = opts.link {
        plot = plot.link_axis(egui::Id::new(group), egui::Vec2b::new(true, false));
    }
    let hlines = opts.hlines;
    plot.show(ui, |p| {
        p.bar_chart(chart);
        for (v, label) in hlines {
            p.hline(egui_plot::HLine::new(label, v).color(Color32::GRAY).style(egui_plot::LineStyle::dashed_loose()));
        }
    });
}

/// Centered moving average (for noisy pace/speed lines).
pub fn smooth(points: &[[f64; 2]], half: usize) -> Vec<[f64; 2]> {
    if half == 0 || points.len() < 3 {
        return points.to_vec();
    }
    let n = points.len();
    let mut prefix = vec![0.0; n + 1];
    for i in 0..n {
        prefix[i + 1] = prefix[i] + points[i][1];
    }
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(n);
            [points[i][0], (prefix[hi] - prefix[lo]) / (hi - lo) as f64]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_keeps_extremes() {
        let pts: Vec<[f64; 2]> = (0..10_000).map(|i| [i as f64, if i == 5_000 { 999.0 } else { 1.0 }]).collect();
        let d = downsample(&pts, 500);
        assert!(d.len() <= 502);
        assert!(d.iter().any(|p| p[1] == 999.0));
        assert!(d.windows(2).all(|w| w[0][0] <= w[1][0]));
    }

    #[test]
    fn smooth_average() {
        let pts = vec![[0.0, 0.0], [1.0, 3.0], [2.0, 0.0]];
        assert_eq!(smooth(&pts, 1)[1], [1.0, 1.0]);
    }

    #[test]
    fn date_axis_roundtrip() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 8, 9).unwrap();
        assert_eq!(Axis::Date.fmt(date_to_x(d)), "09/08");
    }
}
