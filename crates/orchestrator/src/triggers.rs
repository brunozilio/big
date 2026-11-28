use connectors::catalog::namespace;
use connectors::{Connectors, ExternalEvent};
use serde_json::{Value, json};

pub const PLATFORM: [&str; 4] =
    [crate::progress::STEP_FINISHED, crate::factory::ESCAPED, crate::factory::RECURRING, crate::schedules::KIND];

pub const SOURCES: (&str, &str) = ("event-sources.ts", "SOURCES");

pub fn source_of(connectors: &Connectors, event: &ExternalEvent) -> String {
    let declared = event.names().into_iter().find_map(|name| connectors.event(name)).map(|spec| spec.source.clone());
    if let Some(source) = declared.filter(|source| !source.is_empty()) {
        return source;
    }
    let sources = content::value(SOURCES.0, SOURCES.1).unwrap_or_default();
    let spaces = event.names().into_iter().map(namespace).chain([event.capability.as_str()]);
    let found = spaces.into_iter().find_map(|space| sources[space].as_str());
    found.unwrap_or(event.capability.as_str()).to_owned()
}

pub fn platform() -> Vec<Value> {
    PLATFORM.iter().map(|name| json!({ "name": name, "namespace": namespace(name) })).collect()
}

pub fn checked(connectors: &Connectors, described: &Value, workflow: &str) -> Result<(), String> {
    for shape in described["workflows"].as_array().into_iter().flatten() {
        for trigger in shape["triggers"].as_array().into_iter().flatten() {
            let kind = trigger["kind"].as_str().unwrap_or_default();
            known(connectors, kind, workflow).map_err(|why| format!("workflows/{workflow}: {why}"))?;
        }
    }
    Ok(())
}

const TYPO: usize = 2;
const NAMESPACE_TYPO: usize = 1;

fn typo_of<'a>(kind: &str, connectors: &'a Connectors) -> Option<&'a str> {
    let declared = connectors.declared().flat_map(|spec| {
        std::iter::once((spec.name.as_str(), spec.name.as_str()))
            .chain(spec.aliases.iter().map(|alias| (alias.as_str(), alias.as_str())))
    });
    typo_among(kind, PLATFORM.into_iter().map(|name| (name, name)).chain(declared))
}

fn typo_among<'a>(kind: &str, named: impl Iterator<Item = (&'a str, &'a str)>) -> Option<&'a str> {
    let close = |(name, _): &(&str, &str)| {
        domain::command::distance(kind, name) <= TYPO
            && domain::command::distance(namespace(kind), namespace(name)) <= NAMESPACE_TYPO
    };
    let found: Vec<(&str, &str)> = named.filter(close).collect();
    let nearest = domain::command::nearest(kind, found.iter().map(|(name, _)| *name))?;
    found.iter().find(|(name, _)| *name == nearest).map(|(_, suggested)| *suggested)
}

fn known(connectors: &Connectors, kind: &str, workflow: &str) -> Result<(), String> {
    if PLATFORM.contains(&kind) || connectors.declared().any(|spec| spec.answers(kind)) {
        return Ok(());
    }
    match typo_of(kind, connectors) {
        Some(closest) => Err(format!("on('{kind}'): no connector declares this event; the closest is {closest}")),
        None => {
            tracing::warn!(workflow, kind, "no loaded connector declares this event; it fires only if one sends it");
            Ok(())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn named() -> Vec<(&'static str, &'static str)> {
        vec![
            ("github.pr.merged", "github.pr.merged"),
            ("code.pr.merged", "code.pr.merged"),
            ("jira.issue.created", "jira.issue.created"),
            ("tracker.issue.created", "tracker.issue.created"),
        ]
    }

    #[test]
    fn only_a_near_miss_of_a_declared_name_is_a_typo() {
        assert_eq!(typo_among("jira.issue.creatd", named().into_iter()), Some("jira.issue.created"));
        assert_eq!(typo_among("jiar.issue.created", named().into_iter()), Some("jira.issue.created"));
        assert_eq!(typo_among("tracker.issue.creatd", named().into_iter()), Some("tracker.issue.created"));
        assert_eq!(typo_among("gitlab.mr.merged", named().into_iter()), None);
        assert_eq!(typo_among("gitlab.pr.merged", named().into_iter()), None);
        assert_eq!(typo_among("jira.sprint.started", named().into_iter()), None);
        assert_eq!(typo_among("billing.invoice.paid", named().into_iter()), None);
    }
}
