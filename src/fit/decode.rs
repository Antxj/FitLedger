//! Low-level FIT decoder: turns the bytes of a .fit file into a flat list of
//! messages (global number + raw field values), with no knowledge of the
//! profile. `activity.rs` gives meaning to the fields we use.
//!
//! Supports: 12/14-byte headers, chained FIT files, compressed timestamp
//! headers, both byte orders, developer fields (skipped; the original file is
//! kept in the database anyway) and the CRC of each file.
//!
//! Every read is bounds-checked: a malformed file returns an error and never
//! panics (files opened by the user are untrusted input).

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Single unsigned/signed integer (any width).
    Int(i64),
    Float(f64),
    Text(String),
    /// Array field (e.g. `time_in_hr_zone`). Invalid entries stay as None.
    Array(Vec<Option<i64>>),
}

impl Value {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            Value::Float(v) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(*v as f64),
            Value::Float(v) => Some(*v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub global: u16,
    /// (field number, value). Fields holding the "invalid" marker are left out.
    pub fields: Vec<(u8, Value)>,
}

impl Message {
    pub fn get(&self, field: u8) -> Option<&Value> {
        self.fields.iter().find(|(n, _)| *n == field).map(|(_, v)| v)
    }
}

#[derive(Debug, PartialEq)]
pub enum DecodeError {
    NotFit,
    Truncated,
    BadCrc,
    UndefinedLocalType(u8),
    Empty,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DecodeError::NotFit => tr!("não é um arquivo FIT", "not a FIT file"),
            DecodeError::Truncated => tr!("arquivo incompleto (truncado)", "incomplete (truncated) file"),
            DecodeError::BadCrc => tr!("arquivo corrompido (CRC não confere)", "corrupted file (CRC mismatch)"),
            DecodeError::UndefinedLocalType(_) => tr!(
                "arquivo corrompido (mensagem sem definição)",
                "corrupted file (message without definition)"
            ),
            DecodeError::Empty => tr!("arquivo FIT sem dados", "FIT file without data"),
        };
        f.write_str(s)
    }
}

/// Field number used by every message for its timestamp.
pub const TIMESTAMP: u8 = 253;

#[derive(Clone)]
struct FieldDef {
    num: u8,
    size: u8,
    base: u8,
}

#[derive(Clone)]
struct Definition {
    global: u16,
    big_endian: bool,
    fields: Vec<FieldDef>,
    dev_size: usize,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated)?;
        let s = self.data.get(self.pos..end).ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
}

/// Decodes every FIT file contained in `data` (chained files are concatenated).
pub fn decode(data: &[u8]) -> Result<Vec<Message>, DecodeError> {
    if data.len() < 12 {
        return Err(DecodeError::NotFit);
    }
    let mut out = Vec::new();
    let mut start = 0;
    while start < data.len() {
        // trailing zero padding after the last file is tolerated
        if start > 0 && data[start..].iter().all(|&b| b == 0) {
            break;
        }
        start = decode_one(data, start, &mut out)?;
    }
    if out.is_empty() {
        return Err(DecodeError::Empty);
    }
    Ok(out)
}

fn decode_one(data: &[u8], start: usize, out: &mut Vec<Message>) -> Result<usize, DecodeError> {
    let header = data.get(start..).ok_or(DecodeError::Truncated)?;
    if header.len() < 12 {
        return Err(if start == 0 {
            DecodeError::NotFit
        } else {
            DecodeError::Truncated
        });
    }
    let header_size = header[0] as usize;
    if (header_size != 12 && header_size != 14) || &header[8..12] != b".FIT" {
        return Err(DecodeError::NotFit);
    }
    let data_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let records_start = start + header_size;
    let records_end = records_start.checked_add(data_size).ok_or(DecodeError::Truncated)?;
    let crc_end = records_end + 2;
    if crc_end > data.len() {
        return Err(DecodeError::Truncated);
    }
    // The file CRC covers header + records. A value of 0 means "not computed".
    let crc_stored = u16::from_le_bytes([data[records_end], data[records_end + 1]]);
    if crc_stored != 0 && crc16(&data[start..records_end]) != crc_stored {
        return Err(DecodeError::BadCrc);
    }

    let mut r = Reader {
        data: &data[..records_end],
        pos: records_start,
    };
    let mut defs: [Option<Definition>; 16] = Default::default();
    let mut last_timestamp: u32 = 0;

    while r.pos < records_end {
        let h = r.u8()?;
        if h & 0x80 != 0 {
            // compressed timestamp header: data message with a 5-bit time offset
            let local = (h >> 5) & 0x03;
            let offset = (h & 0x1F) as u32;
            let mut ts = (last_timestamp & !0x1F) + offset;
            if offset < (last_timestamp & 0x1F) {
                ts += 0x20;
            }
            last_timestamp = ts;
            let def = defs[local as usize]
                .as_ref()
                .ok_or(DecodeError::UndefinedLocalType(local))?;
            let mut msg = read_data(&mut r, def)?;
            if msg.get(TIMESTAMP).is_none() {
                msg.fields.push((TIMESTAMP, Value::Int(ts as i64)));
            }
            out.push(msg);
            continue;
        }
        let local = h & 0x0F;
        if h & 0x40 != 0 {
            let has_dev = h & 0x20 != 0;
            r.take(1)?; // reserved
            let big_endian = r.u8()? == 1;
            let g = r.take(2)?;
            let global = if big_endian {
                u16::from_be_bytes([g[0], g[1]])
            } else {
                u16::from_le_bytes([g[0], g[1]])
            };
            let n = r.u8()?;
            let mut fields = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let f = r.take(3)?;
                fields.push(FieldDef {
                    num: f[0],
                    size: f[1],
                    base: f[2] & 0x1F,
                });
            }
            let mut dev_size = 0usize;
            if has_dev {
                let nd = r.u8()?;
                for _ in 0..nd {
                    dev_size += r.take(3)?[1] as usize;
                }
            }
            defs[local as usize] = Some(Definition {
                global,
                big_endian,
                fields,
                dev_size,
            });
        } else {
            let def = defs[local as usize]
                .as_ref()
                .ok_or(DecodeError::UndefinedLocalType(local))?;
            let msg = read_data(&mut r, def)?;
            if let Some(ts) = msg.get(TIMESTAMP).and_then(Value::as_i64) {
                last_timestamp = ts as u32;
            }
            out.push(msg);
        }
    }
    Ok(crc_end)
}

fn read_data(r: &mut Reader, def: &Definition) -> Result<Message, DecodeError> {
    let mut fields = Vec::with_capacity(def.fields.len());
    for f in &def.fields {
        let raw = r.take(f.size as usize)?;
        if let Some(v) = parse_value(raw, f.base, def.big_endian) {
            fields.push((f.num, v));
        }
    }
    r.take(def.dev_size)?;
    Ok(Message {
        global: def.global,
        fields,
    })
}

/// (size in bytes, invalid marker) of each base type.
fn base_info(base: u8) -> (usize, u64) {
    match base {
        0x00 => (1, 0xFF),                  // enum
        0x01 => (1, 0x7F),                  // sint8
        0x02 => (1, 0xFF),                  // uint8
        0x03 => (2, 0x7FFF),                // sint16
        0x04 => (2, 0xFFFF),                // uint16
        0x05 => (4, 0x7FFF_FFFF),           // sint32
        0x06 => (4, 0xFFFF_FFFF),           // uint32
        0x08 => (4, 0xFFFF_FFFF),           // float32
        0x09 => (8, u64::MAX),              // float64
        0x0A => (1, 0x00),                  // uint8z
        0x0B => (2, 0x0000),                // uint16z
        0x0C => (4, 0x0000_0000),           // uint32z
        0x0D => (1, 0xFF),                  // byte
        0x0E => (8, 0x7FFF_FFFF_FFFF_FFFF), // sint64
        0x0F => (8, u64::MAX),              // uint64
        0x10 => (8, 0),                     // uint64z
        _ => (1, 0xFF),
    }
}

fn parse_value(raw: &[u8], base: u8, big_endian: bool) -> Option<Value> {
    if base == 0x07 {
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let s = String::from_utf8_lossy(&raw[..end]).trim().to_string();
        return (!s.is_empty()).then_some(Value::Text(s));
    }
    let (size, invalid) = base_info(base);
    if raw.len() < size {
        return None;
    }
    let read = |chunk: &[u8]| -> u64 {
        let mut b = [0u8; 8];
        if big_endian {
            b[8 - size..].copy_from_slice(chunk);
            u64::from_be_bytes(b)
        } else {
            b[..size].copy_from_slice(chunk);
            u64::from_le_bytes(b)
        }
    };
    let convert = |bits: u64| -> Option<i64> {
        if bits == invalid {
            return None;
        }
        Some(match base {
            0x01 => bits as u8 as i8 as i64,
            0x03 => bits as u16 as i16 as i64,
            0x05 => bits as u32 as i32 as i64,
            0x0E => bits as i64,
            _ => bits as i64,
        })
    };
    if base == 0x08 || base == 0x09 {
        let bits = read(&raw[..size]);
        if bits == invalid {
            return None;
        }
        let v = if size == 4 {
            f32::from_bits(bits as u32) as f64
        } else {
            f64::from_bits(bits)
        };
        return v.is_finite().then_some(Value::Float(v));
    }
    let count = raw.len() / size;
    if count == 1 {
        return convert(read(&raw[..size])).map(Value::Int);
    }
    if base == 0x0D {
        // byte arrays are only invalid when every byte is 0xFF
        if raw.iter().all(|&b| b == 0xFF) {
            return None;
        }
    }
    let items: Vec<Option<i64>> = raw.chunks_exact(size).map(|c| convert(read(c))).collect();
    items.iter().any(Option::is_some).then_some(Value::Array(items))
}

/// CRC-16 used by FIT (from the FIT SDK).
pub fn crc16(data: &[u8]) -> u16 {
    const TABLE: [u16; 16] = [
        0x0000, 0xCC01, 0xD801, 0x1400, 0xF001, 0x3C00, 0x2800, 0xE401, 0xA001, 0x6C00, 0x7800, 0xB401, 0x5000, 0x9C01,
        0x8801, 0x4400,
    ];
    let mut crc: u16 = 0;
    for &byte in data {
        let mut tmp = TABLE[(crc & 0xF) as usize];
        crc = (crc >> 4) & 0x0FFF;
        crc = crc ^ tmp ^ TABLE[(byte & 0xF) as usize];
        tmp = TABLE[(crc & 0xF) as usize];
        crc = (crc >> 4) & 0x0FFF;
        crc = crc ^ tmp ^ TABLE[((byte >> 4) & 0xF) as usize];
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::writer::FitWriter;

    #[test]
    fn crc_known_value() {
        // value from the FIT SDK examples ("123456789")
        assert_eq!(crc16(b"123456789"), 0xBB3D);
    }

    #[test]
    fn roundtrip_with_writer() {
        let mut w = FitWriter::new();
        w.define(0, 20, &[(253, 4, 0x86), (3, 1, 0x02), (5, 4, 0x86), (0, 4, 0x85)]);
        w.data(
            0,
            &[
                &1000u32.to_le_bytes(),
                &[150],
                &12345u32.to_le_bytes(),
                &(-5i32).to_le_bytes(),
            ],
        );
        // invalid heart rate (0xFF) is dropped
        w.data(
            0,
            &[
                &1001u32.to_le_bytes(),
                &[0xFF],
                &12400u32.to_le_bytes(),
                &0x7FFF_FFFFi32.to_le_bytes(),
            ],
        );
        let bytes = w.finish();
        let msgs = decode(&bytes).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].global, 20);
        assert_eq!(msgs[0].get(3), Some(&Value::Int(150)));
        assert_eq!(msgs[0].get(0), Some(&Value::Int(-5)));
        assert_eq!(msgs[1].get(3), None);
        assert_eq!(msgs[1].get(0), None);
        assert_eq!(msgs[1].get(5), Some(&Value::Int(12400)));
    }

    #[test]
    fn compressed_timestamps() {
        let mut w = FitWriter::new();
        w.define(0, 20, &[(253, 4, 0x86), (3, 1, 0x02)]);
        w.data(0, &[&0x3Eu32.to_le_bytes(), &[100]]);
        w.define(1, 20, &[(3, 1, 0x02)]);
        // offset 0x01 < last (0x1E): rolls over to 0x41
        w.compressed(1, 0x01, &[&[101]]);
        let msgs = decode(&w.finish()).unwrap();
        assert_eq!(msgs[1].get(TIMESTAMP), Some(&Value::Int(0x41)));
        assert_eq!(msgs[1].get(3), Some(&Value::Int(101)));
    }

    #[test]
    fn corrupted_files_are_errors_not_panics() {
        let mut w = FitWriter::new();
        w.define(0, 20, &[(253, 4, 0x86), (3, 1, 0x02)]);
        for i in 0..50u32 {
            w.data(0, &[&i.to_le_bytes(), &[120]]);
        }
        let good = w.finish();
        assert!(decode(&good).is_ok());

        let mut bad_crc = good.clone();
        let n = bad_crc.len();
        bad_crc[n - 10] ^= 0x55;
        assert_eq!(decode(&bad_crc).unwrap_err(), DecodeError::BadCrc);

        assert_eq!(decode(&good[..good.len() / 2]).unwrap_err(), DecodeError::Truncated);
        assert_eq!(decode(b"hello world, not a fit").unwrap_err(), DecodeError::NotFit);
        assert_eq!(decode(b"").unwrap_err(), DecodeError::NotFit);

        // every prefix and every single-byte corruption must be handled
        for cut in 0..good.len() {
            let _ = decode(&good[..cut]);
        }
        for i in 0..good.len() {
            let mut b = good.clone();
            b[i] = b[i].wrapping_add(0x41);
            let _ = decode(&b);
        }
    }

    #[test]
    fn data_before_definition_is_an_error() {
        let mut w = FitWriter::new();
        w.raw_record(&[0x03, 1, 2, 3]);
        assert!(matches!(decode(&w.finish()), Err(DecodeError::UndefinedLocalType(3))));
    }
}
