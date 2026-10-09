//! Minimal FIT encoder, used only by the tests to build synthetic files.
//! Keeps real activity files (GPS, health data) out of the repository.

use super::decode::crc16;

pub struct FitWriter {
    records: Vec<u8>,
}

impl FitWriter {
    pub fn new() -> Self {
        FitWriter { records: Vec::new() }
    }

    /// Definition message, little-endian. `fields` = (number, size, base type).
    pub fn define(&mut self, local: u8, global: u16, fields: &[(u8, u8, u8)]) {
        self.records.push(0x40 | (local & 0x0F));
        self.records.push(0);
        self.records.push(0);
        self.records.extend_from_slice(&global.to_le_bytes());
        self.records.push(fields.len() as u8);
        for &(num, size, base) in fields {
            self.records.extend_from_slice(&[num, size, base]);
        }
    }

    pub fn data(&mut self, local: u8, values: &[&[u8]]) {
        self.records.push(local & 0x0F);
        for v in values {
            self.records.extend_from_slice(v);
        }
    }

    pub fn compressed(&mut self, local: u8, offset: u8, values: &[&[u8]]) {
        self.records.push(0x80 | ((local & 0x03) << 5) | (offset & 0x1F));
        for v in values {
            self.records.extend_from_slice(v);
        }
    }

    pub fn raw_record(&mut self, bytes: &[u8]) {
        self.records.extend_from_slice(bytes);
    }

    pub fn finish(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.records.len() + 16);
        out.push(14);
        out.push(0x20);
        out.extend_from_slice(&2132u16.to_le_bytes());
        out.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        out.extend_from_slice(b".FIT");
        let hcrc = crc16(&out);
        out.extend_from_slice(&hcrc.to_le_bytes());
        out.extend_from_slice(&self.records);
        let crc = crc16(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        out
    }
}

/// Higher-level helper: a whole synthetic activity in the shape our importer
/// reads (file_id, session, laps, records, events, activity).
pub struct Synthetic {
    pub sport: u8,
    pub sub_sport: u8,
    /// FIT timestamp (seconds since 1989-12-31) of the first record.
    pub start: u32,
    pub serial: u32,
    /// (heart rate, speed m/s, cadence strides/min, altitude m, lat°, lon°) per second.
    pub samples: Vec<(u8, f64, u8, f64, Option<(f64, f64)>)>,
}

impl Synthetic {
    /// Steady run: `secs` seconds at `speed` m/s, heart rate ramping from
    /// `hr0` to `hr1`. Fake GPS around (0°, 0°).
    pub fn run(start: u32, secs: u32, speed: f64, hr0: f64, hr1: f64, gps: bool) -> Self {
        let samples = (0..secs)
            .map(|i| {
                let f = i as f64 / secs.max(1) as f64;
                let hr = (hr0 + (hr1 - hr0) * f).round() as u8;
                let pos = gps.then(|| (0.01, 0.01 + i as f64 * speed / 111_320.0));
                (hr, speed, 82u8, 100.0 + (i as f64 / 60.0).sin() * 5.0, pos)
            })
            .collect();
        Synthetic { sport: 1, sub_sport: if gps { 0 } else { 1 }, start, serial: 4242, samples }
    }

    pub fn encode(&self) -> Vec<u8> {
        let semis = |deg: f64| -> i32 { (deg * (2f64.powi(31) / 180.0)) as i32 };
        let n = self.samples.len() as u32;
        let end = self.start + n.saturating_sub(1);
        let mut w = FitWriter::new();
        // file_id: type, manufacturer, product, serial, time_created, product_name
        w.define(0, 0, &[(0, 1, 0), (1, 2, 0x84), (2, 2, 0x84), (3, 4, 0x8C), (4, 4, 0x86), (8, 16, 7)]);
        let mut name = [0u8; 16];
        name[..13].copy_from_slice(b"Test Watch 1\0");
        w.data(0, &[&[4], &255u16.to_le_bytes(), &1u16.to_le_bytes(), &self.serial.to_le_bytes(), &self.start.to_le_bytes(), &name]);
        // event: timer start
        w.define(1, 21, &[(253, 4, 0x86), (0, 1, 0), (1, 1, 0)]);
        w.data(1, &[&self.start.to_le_bytes(), &[0], &[0]]);
        // records
        w.define(2, 20, &[(253, 4, 0x86), (3, 1, 2), (6, 2, 0x84), (4, 1, 2), (2, 2, 0x84), (5, 4, 0x86), (0, 4, 0x85), (1, 4, 0x85)]);
        let mut dist = 0.0;
        for (i, &(hr, speed, cad, alt, pos)) in self.samples.iter().enumerate() {
            if i > 0 {
                dist += speed;
            }
            let (lat, lon) = pos.map(|(a, b)| (semis(a), semis(b))).unwrap_or((0x7FFF_FFFF, 0x7FFF_FFFF));
            let ts = self.start + i as u32;
            w.data(
                2,
                &[
                    &ts.to_le_bytes(),
                    &[hr],
                    &((speed * 1000.0) as u16).to_le_bytes(),
                    &[cad],
                    &(((alt + 500.0) * 5.0) as u16).to_le_bytes(),
                    &((dist * 100.0) as u32).to_le_bytes(),
                    &lat.to_le_bytes(),
                    &lon.to_le_bytes(),
                ],
            );
        }
        w.data(1, &[&end.to_le_bytes(), &[0], &[4]]);
        let hrs: Vec<f64> = self.samples.iter().map(|s| s.0 as f64).collect();
        let avg_hr = (hrs.iter().sum::<f64>() / hrs.len().max(1) as f64).round() as u8;
        let max_hr = hrs.iter().cloned().fold(0.0, f64::max) as u8;
        let elapsed_ms = n.saturating_sub(1) * 1000;
        let avg_speed = if n > 1 { dist / (n - 1) as f64 } else { 0.0 };
        // lap (one lap covering everything)
        w.define(3, 19, &[(253, 4, 0x86), (2, 4, 0x86), (7, 4, 0x86), (8, 4, 0x86), (9, 4, 0x86), (15, 1, 2), (16, 1, 2)]);
        w.data(3, &[&end.to_le_bytes(), &self.start.to_le_bytes(), &elapsed_ms.to_le_bytes(), &elapsed_ms.to_le_bytes(), &((dist * 100.0) as u32).to_le_bytes(), &[avg_hr], &[max_hr]]);
        // session
        w.define(4, 18, &[(253, 4, 0x86), (2, 4, 0x86), (5, 1, 0), (6, 1, 0), (7, 4, 0x86), (8, 4, 0x86), (9, 4, 0x86), (11, 2, 0x84), (14, 2, 0x84), (16, 1, 2), (17, 1, 2)]);
        w.data(
            4,
            &[
                &end.to_le_bytes(),
                &self.start.to_le_bytes(),
                &[self.sport],
                &[self.sub_sport],
                &elapsed_ms.to_le_bytes(),
                &elapsed_ms.to_le_bytes(),
                &((dist * 100.0) as u32).to_le_bytes(),
                &300u16.to_le_bytes(),
                &((avg_speed * 1000.0) as u16).to_le_bytes(),
                &[avg_hr],
                &[max_hr],
            ],
        );
        // activity with local timestamp (UTC-3)
        w.define(5, 34, &[(253, 4, 0x86), (5, 4, 0x86)]);
        w.data(5, &[&end.to_le_bytes(), &(end - 3 * 3600).to_le_bytes()]);
        w.finish()
    }
}
