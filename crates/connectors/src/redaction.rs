use std::borrow::Cow;

use domain::redact::{Known, Rules};
use serde_json::Value;

use crate::descriptor::ActionSpec;
use crate::provider::Provider;

const CONTENT: &str = "content";
const CONNECTORS: &str = "connectors";

pub fn declare<'a>(providers: impl Iterator<Item = &'a dyn Provider>) {
    let (content, mut problems) = Rules::layered(&content::schema::policies(domain::redact::FILE));
    problems.extend(domain::redact::declare(CONTENT, content));
    let mut declared = Rules::default();
    for provider in providers {
        declared.merge(&provider.secret_patterns());
    }
    problems.extend(domain::redact::declare(CONNECTORS, declared));
    for problem in problems {
        tracing::warn!(%problem, "a redaction rule was left out");
    }
}

pub fn guarded<'a>(spec: &ActionSpec, args: &'a Value) -> Cow<'a, Value> {
    if !spec.profile.acts() {
        return Cow::Borrowed(args);
    }
    let (redacted, found) = domain::redact::current().redact_value(args, &Known::default());
    if found.is_empty() {
        return Cow::Borrowed(args);
    }
    tracing::warn!(action = spec.name, ?found, "secrets were taken out of the arguments before they left");
    Cow::Owned(redacted)
}
