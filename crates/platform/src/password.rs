use std::num::NonZeroU32;

use ring::pbkdf2;

pub const ITERATIONS: u32 = 600_000;
const SCHEME: &str = "pbkdf2-sha256";
const LENGTH: usize = 32;

pub fn hash(password: &str) -> Result<String, String> {
    hash_with(password, ITERATIONS)
}

pub fn hash_with(password: &str, iterations: u32) -> Result<String, String> {
    let rounds = NonZeroU32::new(iterations).ok_or_else(|| "at least one round".to_owned())?;
    let salt = crate::crypto::random(16)?;
    let mut derived = [0u8; LENGTH];
    pbkdf2::derive(pbkdf2::PBKDF2_HMAC_SHA256, rounds, &salt, password.as_bytes(), &mut derived);
    Ok(format!("{SCHEME}${iterations}${}${}", crate::crypto::hex(&salt), crate::crypto::hex(&derived)))
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| text.get(at..at + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok()))
        .collect()
}

pub fn matches(stored: &str, password: &str) -> bool {
    let mut parts = stored.split('$');
    let (Some(SCHEME), Some(rounds), Some(salt), Some(derived), None) =
        (parts.next(), parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Some(rounds) = rounds.parse::<u32>().ok().and_then(NonZeroU32::new) else { return false };
    let (Some(salt), Some(derived)) = (unhex(salt), unhex(derived)) else { return false };
    pbkdf2::verify(pbkdf2::PBKDF2_HMAC_SHA256, rounds, &salt, password.as_bytes(), &derived).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_matches_only_its_own_hash() {
        let stored = hash_with("correct horse", 1_000).unwrap_or_default();
        assert!(stored.starts_with("pbkdf2-sha256$1000$"));
        assert!(matches(&stored, "correct horse"));
        assert!(!matches(&stored, "correct horsE"));
        assert_ne!(stored, hash_with("correct horse", 1_000).unwrap_or_default());
        assert!(!matches("garbage", "x"));
        assert!(!matches("pbkdf2-sha256$0$00$00", "x"));
    }
}
