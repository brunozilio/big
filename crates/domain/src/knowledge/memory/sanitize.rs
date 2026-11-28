pub use crate::redact::REDACTED;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitized(String);

impl Sanitized {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

pub fn sanitize(text: &str) -> Sanitized {
    Sanitized(crate::redact::words(text).join(" "))
}
