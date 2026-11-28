use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ring::rand::SystemRandom;
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};

#[derive(Debug)]
pub struct Signer(Ed25519KeyPair);

impl Signer {
    pub fn generated() -> Result<String, String> {
        let made =
            Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|_| "no key could be made".to_owned())?;
        Ok(STANDARD.encode(made.as_ref()))
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let bytes = STANDARD.decode(text.trim()).map_err(|_| "the signing key is not base64".to_owned())?;
        Ed25519KeyPair::from_pkcs8_maybe_unchecked(&bytes)
            .map(Self)
            .map_err(|_| "the signing key is not an Ed25519 PKCS#8 key".to_owned())
    }

    pub fn sign(&self, message: &[u8]) -> String {
        STANDARD.encode(self.0.sign(message).as_ref())
    }

    pub fn public(&self) -> String {
        STANDARD.encode(self.0.public_key().as_ref())
    }
}

pub fn verified(public: &str, message: &[u8], signature: &str) -> bool {
    let (Ok(public), Ok(signature)) = (STANDARD.decode(public.trim()), STANDARD.decode(signature.trim())) else {
        return false;
    };
    UnparsedPublicKey::new(&ED25519, public).verify(message, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signature_verifies_only_for_its_message_and_key() {
        let key = Signer::generated().unwrap_or_default();
        let signer = Signer::from_text(&key).unwrap_or_else(|_| unreachable!());
        let other = Signer::from_text(&Signer::generated().unwrap_or_default()).unwrap_or_else(|_| unreachable!());
        let signature = signer.sign(b"code");
        assert!(verified(&signer.public(), b"code", &signature));
        assert!(!verified(&signer.public(), b"code!", &signature));
        assert!(!verified(&other.public(), b"code", &signature));
        assert!(Signer::from_text("not a key").is_err());
    }

    #[test]
    fn a_web_crypto_pkcs8_key_is_read() {
        let exported = "MC4CAQAwBQYDK2VwBCIEICYyyK4ttxF8AW3IvcBO5s50hpvFKhiErNoP1lHIC6GF";
        let signer = Signer::from_text(exported).unwrap_or_else(|_| unreachable!());
        assert_eq!(signer.public(), "chMqI9QIhoAEXrDf3b4f44GP/84e07xBNrOhn0e16xo=");
    }
}
