//! Automatic screenshots for the documentation and for checking the UI:
//! `FITLEDGER_SCREENSHOTS=<folder>` opens each page, saves a PNG and closes
//! the app. Use with FITLEDGER_HOME pointing to a folder with fake data.

use std::path::{Path, PathBuf};

pub struct Shooter {
    pub dir: PathBuf,
    pub index: usize,
    pub frames: u32,
    pub requested: bool,
}

impl Shooter {
    pub fn from_env() -> Option<Shooter> {
        let dir = PathBuf::from(std::env::var_os("FITLEDGER_SCREENSHOTS")?);
        let _ = std::fs::create_dir_all(&dir);
        Some(Shooter { dir, index: 0, frames: 0, requested: false })
    }
}

/// Minimal PNG writer (RGBA, stored/uncompressed deflate blocks).
pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = Vec::with_capacity(4 + data.len());
        c.extend_from_slice(kind);
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    }
    // raw scanlines with filter byte 0
    let row = width as usize * 4;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * row..(y + 1) * row]);
    }
    // zlib stream with stored blocks
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    while let Some(b) = blocks.next() {
        z.push(if blocks.peek().is_none() { 1 } else { 0 });
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn png_header() {
        let p = std::env::temp_dir().join(format!("fitledger-png-{}.png", std::process::id()));
        super::write_png(&p, 2, 2, &[255; 16]).unwrap();
        let b = std::fs::read(&p).unwrap();
        assert_eq!(&b[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let _ = std::fs::remove_file(p);
    }
}
