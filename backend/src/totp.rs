//! Time-based one-time passwords (RFC 6238) as used by authenticator apps:
//! HMAC-SHA1, 6 digits, 30-second steps.

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;

const STEP_SECONDS: i64 = 30;
const DIGITS: u32 = 6;
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// A fresh 160-bit secret, base32 encoded the way authenticator apps expect.
pub fn generate_secret() -> String {
    let mut raw = [0u8; 20];
    rand::rngs::OsRng.fill_bytes(&mut raw);
    base32_encode(&raw)
}

pub fn step_at(unix_seconds: i64) -> i64 {
    unix_seconds.div_euclid(STEP_SECONDS)
}

/// The code for one step (RFC 4226 dynamic truncation).
pub fn code_at(secret: &[u8], step: i64) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC takes any key length");
    mac.update(&step.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 0x0f) as usize;
    let value = u32::from_be_bytes([
        digest[offset] & 0x7f,
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]);
    value % 10u32.pow(DIGITS)
}

/// The step a code is valid for, allowing one step of clock drift either way.
pub fn verify(secret_b32: &str, code: &str, now_unix: i64) -> Option<i64> {
    let code = code.trim().replace(' ', "");
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let wanted: u32 = code.parse().ok()?;
    let secret = base32_decode(secret_b32)?;
    let now = step_at(now_unix);
    (now - 1..=now + 1).find(|&step| code_at(&secret, step) == wanted)
}

/// The otpauth:// link an authenticator app reads from the QR code.
pub fn provisioning_uri(secret_b32: &str, username: &str) -> String {
    let label = urlencoding::encode(&format!("Frog:{username}")).into_owned();
    format!(
        "otpauth://totp/{label}?secret={secret_b32}&issuer=Frog&algorithm=SHA1&digits={DIGITS}&period={STEP_SECONDS}"
    )
}

pub fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    for chunk in bytes.chunks(5) {
        let mut buf = [0u8; 5];
        buf[..chunk.len()].copy_from_slice(chunk);
        let bits = u64::from_be_bytes([0, 0, 0, buf[0], buf[1], buf[2], buf[3], buf[4]]);
        let chars = (chunk.len() * 8).div_ceil(5);
        for i in 0..chars {
            let index = (bits >> (35 - i * 5)) & 0x1f;
            out.push(ALPHABET[index as usize] as char);
        }
    }
    out
}

pub fn base32_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let mut buffer: u64 = 0;
    let mut bits = 0;
    for c in text.bytes() {
        if matches!(c, b'=' | b' ' | b'-') {
            continue;
        }
        let value = ALPHABET.iter().position(|&a| a == c.to_ascii_uppercase())? as u64;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 appendix B, SHA-1 key, truncated to six digits.
    #[test]
    fn matches_the_rfc_vectors() {
        let key = b"12345678901234567890";
        assert_eq!(code_at(key, step_at(59)), 287_082);
        assert_eq!(code_at(key, step_at(1_111_111_109)), 81_804);
        assert_eq!(code_at(key, step_at(1_234_567_890)), 5_924);
        assert_eq!(code_at(key, step_at(20_000_000_000)), 353_130);
    }

    #[test]
    fn base32_roundtrips() {
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32_decode("MZXW6YTBOI").unwrap(), b"foobar");
        let secret = generate_secret();
        assert_eq!(secret.len(), 32);
        assert_eq!(base32_decode(&secret).unwrap().len(), 20);
    }

    #[test]
    fn accepts_one_step_of_drift_and_no_more() {
        let secret = base32_encode(b"12345678901234567890");
        let now = 1_111_111_109;
        let code = format!("{:06}", code_at(b"12345678901234567890", step_at(now)));
        assert_eq!(verify(&secret, &code, now), Some(step_at(now)));
        assert!(verify(&secret, &code, now + 30).is_some());
        assert!(verify(&secret, &code, now + 90).is_none());
        assert!(verify(&secret, "12345", now).is_none());
    }
}
