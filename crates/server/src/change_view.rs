use connectors::contracts::PIPELINES;
use domain::workflow::{Found, Output, StepState, WorkItem, text};
use orchestrator::ItemSummary;
use serde_json::{Value, json};

use crate::item_view::{Known, listed_sources, meta};
use crate::marks::{reference, serving};

fn checks_state(state: StepState) -> Value {
    match state {
        StepState::Running | StepState::WaitingHuman => json!("running"),
        StepState::Passed => json!("passed"),
        StepState::Failed => json!("failed"),
        StepState::Pending => json!("pending"),
        StepState::Skipped => Value::Null,
    }
}

fn checks(known: &Known<'_>, output: &Output, summary: &ItemSummary, item: &WorkItem) -> Value {
    let Some(opened) = item.steps.iter().find(|run| text(&run.output[output.field()]).is_some()) else {
        return Value::Null;
    };
    let Some(at) = summary.steps.iter().position(|step| step.id == opened.id) else { return Value::Null };
    summary.steps[at + 1..]
        .iter()
        .find(|step| meta(known, step)["requires"] == PIPELINES)
        .map_or(Value::Null, |step| checks_state(step.state))
}

fn origin(known: &Known<'_>, output: &Output, item: Option<&WorkItem>) -> Value {
    let (Some(kind), Some(via)) = (output.source.as_deref(), output.via.as_deref()) else { return Value::Null };
    let listed = listed_sources(item);
    let Some(source) = listed.iter().find(|source| source["kind"] == kind) else { return Value::Null };
    let key = text(&source["subject"]).unwrap_or_default();
    let chosen = source["connection"].as_str().and_then(|id| known.connections.iter().find(|record| record.id == id));
    let Some(record) = chosen.or_else(|| serving(known.connections, via)) else { return Value::Null };
    let mut value = reference(known.descriptors, record, &key, None);
    value["checks"] = Value::Null;
    value["merged"] = json!(false);
    value
}

fn external(known: &Known<'_>, output: &Output, summary: &ItemSummary, item: &WorkItem, found: &Found) -> Value {
    let Some(record) = output.via.as_deref().and_then(|via| serving(known.connections, via)) else {
        return Value::Null;
    };
    let mut value = reference(known.descriptors, record, &found.key, found.url.as_deref());
    value["checks"] = checks(known, output, summary, item);
    value["merged"] = json!(found.done);
    value
}

pub fn change(known: &Known<'_>, summary: &ItemSummary, item: Option<&WorkItem>) -> Value {
    let Some(output) = known.output else { return Value::Null };
    let found = item.and_then(|item| Some((item, output.found(&item.steps)?)));
    match found {
        Some((item, found)) => external(known, output, summary, item, &found),
        None => origin(known, output, item),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::view_fixtures::{finished, known, ran, record, summary};

    #[test]
    fn a_declared_external_kind_shows_as_the_change_of_the_connection_that_serves_it() {
        let declared =
            json!({ "kind": "change", "key": "ref", "done": { "closed": true }, "source": "opened", "via": "code" });
        let output = Output::read(&declared).ok().flatten();
        let connections = [record("c-code", "forge", "code")];
        let known = known(&connections, output.as_ref());
        let steps = vec![
            ran("open", json!({ "ref": "o/r#7", "url": "https://x/7" })),
            ran("close", json!({ "ref": "o/r#7", "closed": true })),
        ];
        let summary = summary(&["open", "close"]);
        let shown = change(&known, &summary, Some(&finished(steps, None)));
        assert_eq!(
            (shown["key"].clone(), shown["url"].clone(), shown["merged"].clone()),
            (json!("o/r#7"), json!("https://x/7"), json!(true))
        );
        let started = WorkItem {
            input: json!({ "sources": [{ "kind": "opened", "connection": "c-code", "subject": "o/r#3" }] }),
            ..WorkItem::default()
        };
        assert_eq!(change(&known, &summary, Some(&started))["key"], "o/r#3");
    }

    #[test]
    fn a_kind_without_a_connection_or_a_workflow_without_output_shows_no_change() {
        let output = Output::read(&json!({ "kind": "report" })).ok().flatten();
        let connections = [record("c-code", "forge", "code")];
        let item = finished(vec![ran("write", json!({ "report": "parecer.pdf" }))], None);
        let summary = summary(&["write"]);
        assert_eq!(change(&known(&connections, output.as_ref()), &summary, Some(&item)), Value::Null);
        assert_eq!(change(&known(&connections, None), &summary, Some(&item)), Value::Null);
    }
}
