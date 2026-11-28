use std::collections::BTreeMap;

use domain::redact::{Known, REDACTED, Redactor};
use serde_json::{Map, Value};
use store::AuditDraft;

use super::capture::Captured;

const ACTOR: &[&str] = &["agent", "agent.name"];
const SUBJECT: &[&str] = &["user", "person"];
const POLICY: &[&str] = &["policy", "layer", "permission.mode"];
const OUTCOME: &[&str] = &["outcome", "ask.decision", "decision"];
const MOST_CHARS: usize = 2000;

fn clipped(text: &str) -> String {
    match text.char_indices().nth(MOST_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

fn cleaned(redactor: &Redactor, text: &str) -> String {
    clipped(&redactor.redact(text, &Known::default()).text)
}

fn word(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim_matches('"').to_owned()).filter(|text| !text.is_empty()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn taken(fields: &mut BTreeMap<String, Value>, keys: &[&str], redactor: &Redactor) -> Option<String> {
    let found = keys.iter().find_map(|key| fields.remove(*key).as_ref().and_then(word))?;
    for key in keys {
        fields.remove(*key);
    }
    Some(cleaned(redactor, &found))
}

fn detail_value(redactor: &Redactor, key: &str, value: Value) -> Value {
    if redactor.secret_key(key) {
        return Value::String(REDACTED.to_owned());
    }
    match value {
        Value::String(text) => Value::String(cleaned(redactor, &text)),
        other => other,
    }
}

pub fn draft_of(captured: Captured, redactor: &Redactor) -> AuditDraft {
    let Captured { at, level, action, mut fields } = captured;
    let actor = taken(&mut fields, ACTOR, redactor);
    let subject = taken(&mut fields, SUBJECT, redactor);
    let policy = taken(&mut fields, POLICY, redactor);
    let outcome = taken(&mut fields, OUTCOME, redactor);
    let mut detail: Map<String, Value> =
        fields.into_iter().map(|(key, value)| (key.clone(), detail_value(redactor, &key, value))).collect();
    detail.insert("level".to_owned(), Value::String(level.as_str().to_ascii_lowercase()));
    AuditDraft {
        at,
        actor,
        subject,
        action: cleaned(redactor, &action),
        policy,
        outcome,
        detail: Value::Object(detail),
    }
}

pub fn dropped(at: i64, count: u64) -> AuditDraft {
    AuditDraft {
        at,
        action: "audit.dropped".to_owned(),
        outcome: Some("failure".to_owned()),
        detail: serde_json::json!({ "count": count }),
        ..AuditDraft::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::Level;

    fn redactor() -> Redactor {
        let rules = serde_json::json!({ "keys": ["password", "token"], "prefixes": [{ "name": "house", "prefix": "house_", "min": 12 }] });
        Redactor::compiled(domain::redact::Rules::read(&rules).unwrap_or_default()).0
    }

    fn captured(fields: Value) -> Captured {
        let fields = fields.as_object().cloned().unwrap_or_default().into_iter().collect();
        Captured { at: 7, level: Level::WARN, action: "policy.blocked".into(), fields }
    }

    #[test]
    fn the_known_fields_become_actor_subject_policy_and_outcome() {
        let draft = draft_of(
            captured(
                serde_json::json!({ "agent": "dev", "user": "u-ana", "layer": "floor", "outcome": "denied", "tool": "bash" }),
            ),
            &redactor(),
        );
        assert_eq!(draft.actor.as_deref(), Some("dev"));
        assert_eq!(draft.subject.as_deref(), Some("u-ana"));
        assert_eq!(draft.policy.as_deref(), Some("floor"));
        assert_eq!(draft.outcome.as_deref(), Some("denied"));
        assert_eq!(draft.detail, serde_json::json!({ "tool": "bash", "level": "warn" }));
    }

    #[test]
    fn no_secret_reaches_the_trail() {
        let fields = serde_json::json!({
            "db.password": "hunter2-long",
            "error.message": "refused with house_0123456789abcdef",
            "reason": "x".repeat(MOST_CHARS + 10)
        });
        let draft = draft_of(captured(fields), &redactor());
        let shown = draft.detail.to_string();
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(!shown.contains("house_0123456789abcdef"), "{shown}");
        assert_eq!(draft.detail["db.password"], REDACTED);
        assert!(draft.detail["reason"].as_str().is_some_and(|reason| reason.chars().count() == MOST_CHARS + 1));
    }
}
