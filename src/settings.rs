//! User settings, stored as one JSON document in the `settings` table.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follows Windows (light or dark).
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ZoneMethod {
    /// Percentages of max heart rate.
    MaxHr,
    /// Karvonen: percentages of heart rate reserve (max - rest) + rest.
    #[default]
    Reserve,
    /// Bounds typed by the user.
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "pt" | "en"; None follows Windows.
    pub language: Option<String>,
    pub theme: Theme,
    pub age: Option<u32>,
    pub weight_kg: Option<f64>,
    pub height_cm: Option<f64>,
    /// None: estimated from age (or from the highest value seen).
    pub hr_max: Option<f64>,
    pub hr_rest: Option<f64>,
    pub zone_method: ZoneMethod,
    /// Lower bound of zones 1..5 in bpm (Custom method).
    pub custom_zones: [f64; 5],
    /// First-run wizard finished.
    pub onboarded: bool,
    pub auto_update: bool,
    /// Import new files from /fits when the app opens.
    pub auto_import: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: None,
            theme: Theme::System,
            age: None,
            weight_kg: None,
            height_cm: None,
            hr_max: None,
            hr_rest: None,
            zone_method: ZoneMethod::Reserve,
            custom_zones: [100.0, 120.0, 140.0, 160.0, 175.0],
            onboarded: false,
            auto_update: true,
            auto_import: true,
        }
    }
}

/// Percent bounds (lower edge of each zone) used by MaxHr and Reserve.
pub const ZONE_PCTS: [f64; 5] = [0.50, 0.60, 0.70, 0.80, 0.90];

/// Resolved zones: lower bound of zones 1..5 and the max heart rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Zones {
    pub lower: [f64; 5],
    pub max: f64,
}

impl Zones {
    /// Zone (1..=5) of a heart rate; 0 below zone 1.
    pub fn zone_of(&self, hr: f64) -> usize {
        self.lower.iter().rposition(|&lo| hr >= lo).map(|i| i + 1).unwrap_or(0)
    }

    /// (min, max) bpm of zone `z` (1..=5).
    pub fn range(&self, z: usize) -> (f64, f64) {
        let lo = self.lower[z - 1];
        let hi = if z < 5 { self.lower[z] - 1.0 } else { self.max };
        (lo, hi)
    }
}

impl Settings {
    /// Max heart rate to use: typed, else 208 − 0.7 × age (Tanaka 2001) or
    /// 185, raised to the highest recorded value when that is higher.
    pub fn effective_hr_max(&self, observed_max: Option<f64>) -> f64 {
        if let Some(v) = self.hr_max.filter(|&v| v > 100.0) {
            return v;
        }
        let by_age = self.age.map(|a| (208.0 - 0.7 * a as f64).round());
        // a beginner who only ran easily has never reached the real maximum:
        // the highest value seen only counts when it is above the estimate
        let estimate = by_age.unwrap_or(185.0);
        match observed_max.filter(|&v| v > 120.0 && v < 230.0) {
            Some(o) => o.max(estimate),
            None => estimate,
        }
    }

    pub fn effective_hr_rest(&self) -> f64 {
        self.hr_rest.filter(|&v| v > 25.0 && v < 120.0).unwrap_or(60.0)
    }

    pub fn zones(&self, observed_max: Option<f64>) -> Zones {
        let max = self.effective_hr_max(observed_max);
        let lower = match self.zone_method {
            ZoneMethod::MaxHr => ZONE_PCTS.map(|p| (max * p).round()),
            ZoneMethod::Reserve => {
                let rest = self.effective_hr_rest();
                ZONE_PCTS.map(|p| (rest + (max - rest) * p).round())
            }
            ZoneMethod::Custom => {
                let mut z = self.custom_zones;
                // keep the bounds increasing even if typed out of order
                for i in 1..5 {
                    if z[i] <= z[i - 1] {
                        z[i] = z[i - 1] + 1.0;
                    }
                }
                z
            }
        };
        Zones { lower, max }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_by_max_hr() {
        let s = Settings {
            hr_max: Some(200.0),
            zone_method: ZoneMethod::MaxHr,
            ..Default::default()
        };
        let z = s.zones(None);
        assert_eq!(z.lower, [100.0, 120.0, 140.0, 160.0, 180.0]);
        assert_eq!(z.zone_of(99.0), 0);
        assert_eq!(z.zone_of(100.0), 1);
        assert_eq!(z.zone_of(159.0), 3);
        assert_eq!(z.zone_of(185.0), 5);
        assert_eq!(z.range(2), (120.0, 139.0));
        assert_eq!(z.range(5), (180.0, 200.0));
    }

    #[test]
    fn zones_by_reserve() {
        let s = Settings {
            hr_max: Some(190.0),
            hr_rest: Some(60.0),
            zone_method: ZoneMethod::Reserve,
            ..Default::default()
        };
        // 60 + 130 * 0.5 = 125
        assert_eq!(s.zones(None).lower[0], 125.0);
    }

    #[test]
    fn hr_max_fallbacks() {
        let s = Settings::default();
        assert_eq!(s.effective_hr_max(None), 185.0);
        assert_eq!(s.effective_hr_max(Some(196.0)), 196.0);
        // easy runs only: the low observed value does not drag the zones down
        assert_eq!(s.effective_hr_max(Some(150.0)), 185.0);
        let s = Settings {
            age: Some(40),
            ..Default::default()
        };
        assert_eq!(s.effective_hr_max(None), 180.0);
        assert_eq!(s.effective_hr_max(Some(196.0)), 196.0);
    }

    #[test]
    fn old_json_still_loads() {
        let s: Settings = serde_json::from_str(r#"{"age": 35, "unknown_field": 1}"#).unwrap();
        assert_eq!(s.age, Some(35));
        assert!(s.auto_update);
    }
}
