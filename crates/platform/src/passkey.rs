use ring::signature::{self, UnparsedPublicKey, VerificationAlgorithm};

pub const ES256: i64 = -7;
pub const EDDSA: i64 = -8;
pub const RS256: i64 = -257;

pub const SUPPORTED: &[i64] = &[ES256, EDDSA, RS256];

const USER_PRESENT: u8 = 0x01;
const USER_VERIFIED: u8 = 0x04;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authenticator {
    pub rp_id_hash: Vec<u8>,
    pub flags: u8,
    pub counter: u32,
}

impl Authenticator {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let rp_id_hash = data.get(..32)?.to_vec();
        let flags = *data.get(32)?;
        let counter = u32::from_be_bytes(data.get(33..37)?.try_into().ok()?);
        Some(Self { rp_id_hash, flags, counter })
    }

    pub fn present(&self) -> bool {
        self.flags & USER_PRESENT != 0
    }

    pub fn verified(&self) -> bool {
        self.flags & USER_VERIFIED != 0
    }
}

fn element(der: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let tag = *der.first()?;
    let first = *der.get(1)?;
    let (length, header) = if first & 0x80 == 0 {
        (usize::from(first), 2)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 {
            return None;
        }
        let bytes = der.get(2..2 + count)?;
        (bytes.iter().fold(0usize, |total, byte| (total << 8) | usize::from(*byte)), 2 + count)
    };
    let body = der.get(header..header.checked_add(length)?)?;
    Some((tag, body, der.get(header + length..)?))
}

pub fn spki_key(der: &[u8]) -> Option<Vec<u8>> {
    let (outer, body, _) = element(der)?;
    if outer != 0x30 {
        return None;
    }
    let (algorithm, _, rest) = element(body)?;
    let (bits, key, _) = element(rest)?;
    if algorithm != 0x30 || bits != 0x03 || key.first() != Some(&0) {
        return None;
    }
    Some(key.get(1..)?.to_vec())
}

fn algorithm(id: i64) -> Option<&'static dyn VerificationAlgorithm> {
    match id {
        ES256 => Some(&signature::ECDSA_P256_SHA256_ASN1),
        EDDSA => Some(&signature::ED25519),
        RS256 => Some(&signature::RSA_PKCS1_2048_8192_SHA256),
        _ => None,
    }
}

pub fn verify(algorithm_id: i64, spki: &[u8], message: &[u8], signature: &[u8]) -> bool {
    let (Some(algorithm), Some(key)) = (algorithm(algorithm_id), spki_key(spki)) else { return false };
    UnparsedPublicKey::new(algorithm, key).verify(message, signature).is_ok()
}

pub fn signed_message(authenticator_data: &[u8], client_data_json: &[u8]) -> Vec<u8> {
    let mut message = authenticator_data.to_vec();
    message.extend(crate::crypto::sha256(client_data_json));
    message
}

#[cfg(test)]
#[path = "passkey_tests.rs"]
mod tests;
