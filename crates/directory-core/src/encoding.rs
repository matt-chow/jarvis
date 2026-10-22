//! Byte encodings shared across modules.

use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use sha2::{Digest, Sha256};

pub fn b64url_encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeError;

/// Accepts base64url with or without padding.
pub fn b64url_decode(s: &str) -> Result<Vec<u8>, DecodeError> {
    let engine = if s.ends_with('=') {
        URL_SAFE
    } else {
        URL_SAFE_NO_PAD
    };
    engine.decode(s).map_err(|_| DecodeError)
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0xf) as usize] as char);
    }
    out
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&sha256(bytes))
}

/// Length-checked comparison that does not short-circuit on content.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_round_trip_with_and_without_padding() {
        let data = b"\xfb\xff hello";
        let enc = b64url_encode(data);
        assert!(!enc.contains('=') && !enc.contains('+') && !enc.contains('/'));
        assert_eq!(b64url_decode(&enc).unwrap(), data);
        assert_eq!(b64url_decode(&format!("{enc}=")).unwrap(), data);
        assert!(b64url_decode("not base64!").is_err());
    }

    #[test]
    fn hashes_and_compares() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"diff"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }
}
