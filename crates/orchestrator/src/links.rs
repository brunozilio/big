use std::collections::BTreeSet;

use domain::effect::Call;
use domain::workflow::{ItemId, WorkItem, WorkflowEvent};
use serde_json::{Value, json};

use crate::agent::Failure;
use crate::shared::Shared;
use crate::views::item_json;

pub const CAPABILITY: &str = "items";
pub const LINK: &str = "items.link";
pub const UNLINK: &str = "items.unlink";
pub const ACCEPT: &str = "items.accept";
pub const GET: &str = "items.get";
pub const UNCLAIMED: &str = "items.unclaimed";

fn failure(error: impl std::fmt::Display) -> Failure {
    Failure::permanent(error.to_string())
}

fn text<'a>(call: &'a Call, name: &str) -> Option<&'a str> {
    call.args.get(name).and_then(Value::as_str).filter(|text| !text.is_empty())
}

fn member(shared: &Shared, call: &Call) -> Result<ItemId, Failure> {
    let id = ItemId::from(text(call, "item").ok_or_else(|| Failure::permanent("the argument `item` is missing"))?);
    shared.item(&id).map_err(failure)?;
    Ok(id)
}

pub fn application_of(item: &WorkItem) -> Value {
    item.input["application"]["id"].clone()
}

fn state_name(item: &WorkItem, step: &str) -> Option<String> {
    let run = item.steps.iter().rev().find(|run| run.id == step)?;
    serde_json::to_value(run.state).ok().and_then(|value| value.as_str().map(str::to_owned))
}

fn text_at(value: &Value) -> Option<String> {
    value.as_str().map(str::trim).filter(|text| !text.is_empty()).map(str::to_owned)
}

pub fn base_of(item: &WorkItem) -> Option<String> {
    let given = crate::git::chosen(item).into_iter().find_map(|wanted| wanted.base);
    given.or_else(|| {
        item.steps.iter().rev().find_map(|run| {
            text_at(&run.output["base"])
                .or_else(|| run.output["prs"].as_array().and_then(|prs| prs.iter().find_map(|pr| text_at(&pr["base"]))))
        })
    })
}

pub fn integrated(item: &WorkItem) -> bool {
    let fallback = base_of(item);
    item.input["git"].as_object().into_iter().flatten().any(|(repo, entry)| {
        let Some(lane) = entry["promote"].get(0).and_then(|first| text_at(&first["from"])) else { return false };
        crate::git::base_of(item, repo).or_else(|| fallback.clone()).is_some_and(|base| base == lane)
    })
}

pub fn summary(item: &WorkItem) -> Value {
    let mut value = item_json(item);
    let steps: Vec<Value> =
        item.steps.iter().map(|run| json!({ "id": run.id, "state": run.state, "output": run.output })).collect();
    value["steps"] = json!(steps);
    value["ended"] = json!(item.is_ended());
    value["status"] = json!(item.status);
    value["base"] = json!(base_of(item));
    value
}

pub fn claimed_by_peers(all: &[WorkItem], workflow: &str) -> BTreeSet<ItemId> {
    all.iter()
        .filter(|peer| peer.workflow == workflow && !peer.is_cancelled())
        .flat_map(|peer| peer.links.iter().chain(peer.released.iter()).cloned())
        .collect()
}

pub fn unclaimed(
    me: &WorkItem,
    all: &[WorkItem],
    step: &str,
    state: &str,
    base: Option<&str>,
    lane: bool,
) -> Vec<ItemId> {
    let taken = claimed_by_peers(all, &me.workflow);
    all.iter()
        .filter(|other| other.id != me.id && other.workflow != me.workflow && !other.is_cancelled())
        .filter(|other| application_of(other) == application_of(me))
        .filter(|other| state_name(other, step).as_deref() == Some(state))
        .filter(|other| !taken.contains(&other.id))
        .filter(|other| match base {
            Some(wanted) => base_of(other).as_deref() == Some(wanted) || (lane && integrated(other)),
            None => !lane || integrated(other),
        })
        .map(|other| other.id.clone())
        .collect()
}

pub fn call(shared: &Shared, item: &ItemId, call: &Call) -> Result<Value, Failure> {
    match call.action.as_str() {
        LINK => {
            let member = member(shared, call)?;
            shared.append(item, &[WorkflowEvent::Linked { item: member.clone() }]).map_err(failure)?;
            Ok(json!({ "item": member }))
        }
        UNLINK => {
            let member = member(shared, call)?;
            shared.append(item, &[WorkflowEvent::Unlinked { item: member.clone() }]).map_err(failure)?;
            Ok(json!({ "item": member }))
        }
        ACCEPT => {
            let on = call.args["on"].as_bool().unwrap_or(true);
            shared.append(item, &[WorkflowEvent::Accepting { on }]).map_err(failure)?;
            Ok(json!({ "on": on }))
        }
        GET => {
            let wanted = call.args["items"].as_array().cloned().unwrap_or_default();
            let found: Vec<Value> = wanted
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|id| shared.item(&ItemId::from(id)).ok())
                .map(|found| summary(&found))
                .collect();
            Ok(json!(found))
        }
        UNCLAIMED => {
            let me = shared.item(item).map_err(failure)?;
            let all = shared.store.items().map_err(failure)?;
            let step = text(call, "step").ok_or_else(|| Failure::permanent("the argument `step` is missing"))?;
            let state = text(call, "state").unwrap_or("passed");
            let lane = call.args["integrated"].as_bool().unwrap_or(false);
            Ok(json!(unclaimed(&me, &all, step, state, text(call, "base"), lane)))
        }
        other => Err(Failure::permanent(format!("the kernel does not offer {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use domain::workflow::{ItemId, StepRun, StepState, WorkItem};
    use serde_json::json;

    use super::unclaimed;

    fn merged(id: &str, git: serde_json::Value) -> WorkItem {
        let run = StepRun { id: "merge".into(), state: StepState::Passed, ..StepRun::default() };
        WorkItem {
            id: ItemId::from(id),
            workflow: "default".into(),
            input: json!({ "application": { "id": "app" }, "git": git }),
            steps: vec![run],
            ..WorkItem::default()
        }
    }

    #[test]
    fn without_a_fixed_base_the_package_takes_only_what_landed_where_the_flow_promotes_from() {
        let promote = json!([{ "from": "develop", "into": "release" }]);
        let package = WorkItem {
            id: ItemId::from("p"),
            workflow: "package".into(),
            input: json!({ "application": { "id": "app" } }),
            ..WorkItem::default()
        };
        let all = vec![
            package.clone(),
            merged("feature", json!({ "web": { "base": "develop", "promote": promote } })),
            merged("hotfix", json!({ "web": { "base": "main", "promote": promote } })),
            merged("trunk", json!({ "api": { "base": "trunk", "promote": [{ "from": "trunk", "into": "prod" }] } })),
        ];
        let found = unclaimed(&package, &all, "merge", "passed", None, true);
        assert_eq!(found, vec![ItemId::from("feature"), ItemId::from("trunk")]);
        assert_eq!(unclaimed(&package, &all, "merge", "passed", None, false).len(), 3);
    }
}
