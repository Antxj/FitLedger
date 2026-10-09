//! SHA-256 through Windows' own BCrypt (no extra dependency).

use std::ptr;

pub fn sha256_hex(data: &[u8]) -> Option<String> {
    use windows_sys::Win32::Security::Cryptography::{BCRYPT_SHA256_ALG_HANDLE, BCryptHash};
    let mut out = [0u8; 32];
    // SAFETY: BCryptHash only reads `data` (pointer + length of a live slice)
    // and writes exactly `out.len()` bytes into `out`. The pseudo-handle
    // BCRYPT_SHA256_ALG_HANDLE needs no opening or closing.
    let status = unsafe {
        BCryptHash(
            BCRYPT_SHA256_ALG_HANDLE,
            ptr::null(),
            0,
            data.as_ptr(),
            u32::try_from(data.len()).ok()?,
            out.as_mut_ptr(),
            out.len() as u32,
        )
    };
    (status == 0).then(|| out.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn known_value() {
        assert_eq!(
            super::sha256_hex(b"abc").as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
    }
}
