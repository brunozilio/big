use std::collections::BTreeSet;

use domain::workflow::{Binding, WorkItem, WorkflowEvent};
use serde_json::Value;

pub const LANE: &str = Binding::LANE;
pub const BASE: &str = Binding::BASE;

pub fn lane(item: &WorkItem) -> Option<String> {
    item.bound("", LANE).and_then(Binding::text).map(str::to_owned)
}

pub fn named_lane(name: String) -> WorkflowEvent {
    WorkflowEvent::Bound { binding: Binding::new("", LANE, Value::String(name)), keep: true }
}

pub fn chosen_base(repo: &str, base: String) -> WorkflowEvent {
    WorkflowEvent::Bound { binding: Binding::new(repo, BASE, Value::String(base)), keep: false }
}

pub fn wanted(item: &WorkItem, repo: &str, name: &str) -> Option<String> {
    if let Some(bound) = item.bound(repo, name).and_then(Binding::text) {
        return Some(bound.to_owned());
    }
    let text = item.input["git"][repo][name].as_str().map(str::trim)?;
    (!text.is_empty()).then(|| text.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub repository: String,
    pub base: Option<String>,
    pub head: Option<String>,
}

pub fn chosen(item: &WorkItem) -> Vec<Wanted> {
    let given = item.input["git"].as_object().into_iter().flat_map(|all| all.keys().cloned());
    let bound = item.bindings.iter().filter(|binding| binding.kind == BASE).map(|binding| binding.connection.clone());
    let repositories: BTreeSet<String> = given.chain(bound).collect();
    repositories
        .into_iter()
        .map(|repository| {
            let pick = |name: &str| wanted(item, &repository, name);
            Wanted { base: pick(BASE), head: pick("head"), repository }
        })
        .collect()
}
