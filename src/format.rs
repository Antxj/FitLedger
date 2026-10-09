//! Number, pace, duration and date formatting in the current language,
//! plus the pace ↔ speed conversions.

use chrono::{Datelike, NaiveDate, NaiveDateTime};

/// Decimal number with the separator of the current language.
pub fn num(v: f64, decimals: usize) -> String {
    let mut s = format!("{v:.decimals$}");
    // "-0,0" → "0,0"
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        s.remove(0);
    }
    if crate::i18n::pt() { s.replace('.', ",") } else { s }
}

/// Like `num` but drops a trailing ",0" ("10,0" stays only when asked).
pub fn num_trim(v: f64, decimals: usize) -> String {
    let s = num(v, decimals);
    if s.contains([',', '.']) {
        s.trim_end_matches('0').trim_end_matches([',', '.']).to_string()
    } else {
        s
    }
}

/// Integer with thousands separator: "51.322" (pt) / "51,322" (en).
pub fn int(v: f64) -> String {
    let n = v.round() as i64;
    let digits = n.unsigned_abs().to_string();
    let sep = if crate::i18n::pt() { '.' } else { ',' };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

pub fn km(meters: f64) -> String {
    format!("{} km", num(meters / 1000.0, 2))
}

pub fn opt_km(meters: Option<f64>) -> String {
    meters.filter(|&m| m > 0.0).map(km).unwrap_or_else(dash)
}

pub fn dash() -> String {
    "—".to_string()
}

/// "6:05" for 365 s/km.
pub fn pace(s_per_km: f64) -> String {
    if !s_per_km.is_finite() || s_per_km <= 0.0 || s_per_km > 3600.0 {
        return dash();
    }
    let total = s_per_km.round() as i64;
    format!("{}:{:02}", total / 60, total % 60)
}

pub fn pace_unit(s_per_km: f64) -> String {
    let p = pace(s_per_km);
    if p == dash() { p } else { format!("{p}/km") }
}

pub fn opt_pace(s_per_km: Option<f64>) -> String {
    s_per_km.map(pace_unit).unwrap_or_else(dash)
}

/// "1:02:03" or "30:24".
pub fn duration(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return dash();
    }
    let total = secs.round() as i64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn opt_duration(secs: Option<f64>) -> String {
    secs.map(duration).unwrap_or_else(dash)
}

/// "5h10" / "45 min" — totals on cards.
pub fn hours_short(secs: f64) -> String {
    let mins = (secs / 60.0).round() as i64;
    if mins < 60 {
        format!("{mins} min")
    } else {
        format!("{}h{:02}", mins / 60, mins % 60)
    }
}

pub fn bpm(v: Option<f64>) -> String {
    v.map(|v| format!("{} bpm", v.round() as i64)).unwrap_or_else(dash)
}

/// Pace (s/km) → speed (km/h). 360 s/km = 10 km/h.
pub fn pace_to_kmh(s_per_km: f64) -> f64 {
    3600.0 / s_per_km
}

/// Speed (km/h) → pace (s/km).
pub fn kmh_to_pace(kmh: f64) -> f64 {
    3600.0 / kmh
}

/// Reads "6:00", "6'00", "6.5" (minutes) or "360" (seconds) as s/km.
pub fn parse_pace(text: &str) -> Option<f64> {
    let t = text.trim().trim_end_matches("/km").trim();
    if let Some((m, s)) = t.split_once([':', '\'']) {
        let m: f64 = m.trim().parse().ok()?;
        let s: f64 = s.trim().trim_end_matches('"').parse().ok()?;
        if !(0.0..60.0).contains(&s) {
            return None;
        }
        let v = m * 60.0 + s;
        return (v > 0.0).then_some(v);
    }
    let v = parse_num(t)?;
    if v <= 0.0 {
        return None;
    }
    // small numbers are minutes ("6.5"), larger are seconds
    Some(if v < 30.0 { v * 60.0 } else { v })
}

/// Number typed by the user, accepting comma or dot.
pub fn parse_num(text: &str) -> Option<f64> {
    let v: f64 = text.trim().replace(',', ".").parse().ok()?;
    v.is_finite().then_some(v)
}

/// "1:02:03", "30:24" or "45" (minutes) → seconds.
pub fn parse_duration(text: &str) -> Option<f64> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    let nums: Option<Vec<f64>> = parts.iter().map(|p| p.trim().parse::<f64>().ok()).collect();
    let n = nums?;
    let v = match n.as_slice() {
        [m] => m * 60.0,
        [m, s] => m * 60.0 + s,
        [h, m, s] => h * 3600.0 + m * 60.0 + s,
        _ => return None,
    };
    (v >= 0.0).then_some(v)
}

pub fn date(d: NaiveDate) -> String {
    if crate::i18n::pt() {
        d.format("%d/%m/%Y").to_string()
    } else {
        d.format("%Y-%m-%d").to_string()
    }
}

pub fn date_short(d: NaiveDate) -> String {
    if crate::i18n::pt() {
        d.format("%d/%m").to_string()
    } else {
        d.format("%m-%d").to_string()
    }
}

pub fn datetime(d: NaiveDateTime) -> String {
    format!("{} {}", date(d.date()), d.format("%H:%M"))
}

pub fn weekday_short(d: NaiveDate) -> &'static str {
    let i = d.weekday().num_days_from_monday() as usize;
    if crate::i18n::pt() {
        ["SEG", "TER", "QUA", "QUI", "SEX", "SÁB", "DOM"][i]
    } else {
        ["MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN"][i]
    }
}

pub fn month_name(month: u32) -> &'static str {
    let i = (month.clamp(1, 12) - 1) as usize;
    if crate::i18n::pt() {
        [
            "Janeiro",
            "Fevereiro",
            "Março",
            "Abril",
            "Maio",
            "Junho",
            "Julho",
            "Agosto",
            "Setembro",
            "Outubro",
            "Novembro",
            "Dezembro",
        ][i]
    } else {
        [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ][i]
    }
}

/// Signed percentage: "+8%" / "-3%".
pub fn pct_signed(v: f64) -> String {
    format!("{}{}%", if v > 0.0 { "+" } else { "" }, num(v, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pace_speed_examples_from_the_spec() {
        // 7:30/km = 8,0 km/h ... 5:00/km = 12,0 km/h
        let cases = [
            ("7:30", "8,00"),
            ("7:00", "8,57"),
            ("6:30", "9,23"),
            ("6:00", "10,00"),
            ("5:30", "10,91"),
            ("5:00", "12,00"),
        ];
        for (p, k) in cases {
            let s = parse_pace(p).unwrap();
            assert_eq!(num(pace_to_kmh(s), 2), k, "{p}");
            assert_eq!(pace(kmh_to_pace(pace_to_kmh(s))), p);
        }
    }

    #[test]
    fn formatting() {
        assert_eq!(pace(365.0), "6:05");
        assert_eq!(pace(359.6), "6:00");
        assert_eq!(pace(0.0), "—");
        assert_eq!(duration(1824.0), "30:24");
        assert_eq!(duration(4569.0), "1:16:09");
        assert_eq!(hours_short(18600.0), "5h10");
        assert_eq!(hours_short(2700.0), "45 min");
        assert_eq!(km(5025.0), "5,03 km");
        assert_eq!(num_trim(10.0, 2), "10");
        assert_eq!(num_trim(10.5, 2), "10,5");
        assert_eq!(pct_signed(8.0), "+8%");
        assert_eq!(int(51322.4), "51.322");
        assert_eq!(int(999.0), "999");
        assert_eq!(int(-1234567.0), "-1.234.567");
    }

    #[test]
    fn parsing() {
        assert_eq!(parse_pace("6:00"), Some(360.0));
        assert_eq!(parse_pace("6'15"), Some(375.0));
        assert_eq!(parse_pace("6:00/km"), Some(360.0));
        assert_eq!(parse_pace("6,5"), Some(390.0));
        assert_eq!(parse_pace("6:75"), None);
        assert_eq!(parse_pace("abc"), None);
        assert_eq!(parse_num("10,9"), Some(10.9));
        assert_eq!(parse_duration("30:24"), Some(1824.0));
        assert_eq!(parse_duration("1:00:00"), Some(3600.0));
        assert_eq!(parse_duration("45"), Some(2700.0));
        assert_eq!(parse_duration("x"), None);
    }
}
