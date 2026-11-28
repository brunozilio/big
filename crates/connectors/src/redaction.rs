use std::sync::Arc;

use domain::redact::{FILE, Known, Redactor, Rules, Severity};
use serde_json::Value;

use crate::descriptor::ActionSpec;
use crate::error::ActionError;
use crate::provider::Provider;

pub fn redactor<'a>(providers: impl Iterator<Item = &'a dyn Provider>) -> Arc<Redactor> {
    let declared: Vec<Rules> = providers.map(Provider::secret_patterns).collect();
    let shipped = content::builtin_text(FILE);
    let mine = content::user_text(content::folder().as_deref(), FILE);
    let (redactor, problems) = domain::redact::assemble(shipped.as_deref(), mine.as_deref(), &declared);
    for problem in problems {
        match problem.severity {
            Severity::Error => tracing::error!(%problem, "redaction rules"),
            Severity::Warning => tracing::warn!(%problem, "redaction rules"),
        }
    }
    Arc::new(redactor)
}

fn carries_a_secret(redactor: &Redactor, value: &Value) -> bool {
    redactor.finds_in(value, &Known::default())
}

pub fn guarded(redactor: &Redactor, spec: &ActionSpec, args: &Value) -> Result<(), ActionError> {
    if !spec.profile.acts() {
        return Ok(());
    }
    let meant = |field: &str| spec.secret_fields.iter().any(|secret| secret == field);
    let found = match args {
        Value::Object(fields) => fields
            .iter()
            .find(|(field, value)| !meant(field) && carries_a_secret(redactor, value))
            .map(|(field, _)| field.clone()),
        other => carries_a_secret(redactor, other).then(String::new),
    };
    match found {
        Some(field) => {
            tracing::warn!(action = spec.name, field, "a write carried a secret outside its secret fields");
            Err(ActionError::secret_in(&field))
        }
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_rules_load_and_are_never_empty() {
        let (rules, problem) = domain::redact::shipped(content::builtin_text(FILE).as_deref());
        assert_eq!(problem, None);
        assert!(!rules.prefixes.is_empty() && !rules.patterns.is_empty() && !rules.keys.is_empty(), "{rules:?}");
        assert!(rules.jwt && !rules.labels.is_empty() && rules.value_min.is_some() && !rules.type_names.is_empty());
        assert_eq!(rules.entropy, None, "entropy stays off until content turns it on");
    }
}
