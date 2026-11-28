use std::path::Path;
use std::sync::OnceLock;

use platform::signing::Signer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub signature: String,
    pub key: String,
}

static SIGNED: OnceLock<Option<Signed>> = OnceLock::new();

pub fn prepare(data: &Path) -> Option<&'static Signed> {
    SIGNED
        .get_or_init(|| match made(data) {
            Ok(signed) => Some(signed),
            Err(error) => {
                tracing::warn!(%error, "the browser logic could not be signed; paired browsers will refuse it");
                None
            }
        })
        .as_ref()
}

pub fn signed() -> Option<&'static Signed> {
    SIGNED.get().and_then(Option::as_ref)
}

pub fn signature_of(key: &str) -> Result<Signed, String> {
    let signer = Signer::from_text(key)?;
    Ok(Signed { signature: signer.sign(browser::logic().code.as_bytes()), key: signer.public() })
}

fn made(data: &Path) -> Result<Signed, String> {
    let signature = brand::browser::LOGIC_SIGNATURE_FILE;
    if !signature.is_empty() {
        return Ok(Signed {
            signature: read(Path::new(signature))?,
            key: read(Path::new(brand::browser::LOGIC_KEY_FILE))?,
        });
    }
    signature_of(&local_key(&data.join(brand::browser::LOGIC_SIGNING_KEY))?)
}

fn local_key(path: &Path) -> Result<String, String> {
    if let Ok(kept) = std::fs::read_to_string(path) {
        return Ok(kept);
    }
    let made = Signer::generated()?;
    platform::private_file::write(path, made.as_bytes()).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(made)
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map(|text| text.trim().to_owned())
        .map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_local_key_is_made_once_and_signs_the_served_logic() {
        let data = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let first = made(data.path()).unwrap_or_else(|error| panic!("{error}"));
        let again = made(data.path()).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(first, again);
        assert!(platform::signing::verified(&first.key, browser::logic().code.as_bytes(), &first.signature));
    }
}
