use serde::Serialize;

use super::{Levels, Read, Trust};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Held {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub untrusted: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acted: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Next<'a> {
    pub tool: &'a str,
    pub untrusted: bool,
    pub sensitive: bool,
    pub acts: bool,
}

fn first(slot: &mut Option<String>, yes: bool, source: &str) {
    if yes && slot.is_none() {
        *slot = Some(source.to_owned());
    }
}

impl Held {
    pub fn of<'a>(levels: &Levels, reads: impl IntoIterator<Item = &'a Read>) -> Held {
        let mut held = Held::default();
        for read in reads {
            first(&mut held.untrusted, read.trust == Trust::External, &read.source);
            first(&mut held.sensitive, read.sensitive || levels.sensitive(read.label.as_deref()), &read.source);
            first(&mut held.acted, read.acted, &read.source);
        }
        held
    }

    pub fn all(&self) -> bool {
        self.untrusted.is_some() && self.sensitive.is_some() && self.acted.is_some()
    }

    pub fn with(&self, next: &Next<'_>) -> Held {
        let mut joined = self.clone();
        first(&mut joined.untrusted, next.untrusted, next.tool);
        first(&mut joined.sensitive, next.sensitive, next.tool);
        first(&mut joined.acted, next.acts, next.tool);
        joined
    }

    pub fn joined(&self, next: &Next<'_>) -> Option<String> {
        let touches = next.untrusted || next.sensitive || next.acts;
        let joined = self.with(next);
        if !touches || !joined.all() {
            return None;
        }
        let untrusted = joined.untrusted.as_deref().unwrap_or_default();
        let sensitive = joined.sensitive.as_deref().unwrap_or_default();
        let acted = joined.acted.as_deref().unwrap_or_default();
        Some(format!(
            "would join, in one session, untrusted content (from {untrusted}), sensitive data (from {sensitive}) and a change or send (by {acted})"
        ))
    }
}
