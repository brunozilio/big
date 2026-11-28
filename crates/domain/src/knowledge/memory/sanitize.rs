use super::MemoryEdit;
pub use crate::redact::REDACTED;
use crate::redact::Redactor;

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

pub fn sanitize(text: &str, redactor: &Redactor) -> Sanitized {
    Sanitized(redactor.words(text).join(" "))
}

impl MemoryEdit {
    pub fn redacted(&self, redactor: &Redactor) -> MemoryEdit {
        let mut edit = self.clone();
        if let MemoryEdit::Remember { text, .. } | MemoryEdit::Revise { text, .. } = &mut edit {
            *text = sanitize(text, redactor).into_inner();
        }
        edit
    }
}
