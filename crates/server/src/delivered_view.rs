use domain::workflow::{Delivered, Ending, Found, Output, StepState, WorkItem, text};
use orchestrator::ItemSummary;
use serde_json::{Value, json};

use crate::item_view::{Known, listed_sources, meta};
use crate::marks::{iso, reference, serving};

const PIPELINES: &str = "pipelines";

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

fn kept(item: &WorkItem) -> Option<&Delivered> {
    match &item.ending {
        Some(Ending::Finished { delivered, .. }) => delivered.as_ref(),
        _ => None,
    }
}

pub fn delivered(known: &Known<'_>, summary: &ItemSummary, item: Option<&WorkItem>) -> Value {
    let Some(item) = item.filter(|item| item.is_finished()) else { return Value::Null };
    let at = summary.ended_at.unwrap_or(summary.updated_at);
    let found = kept(item).cloned().or_else(|| known.output.and_then(|output| output.deliver(&item.steps, at)));
    let Some(found) = found else { return Value::Null };
    json!({
        "kind": found.kind,
        "key": found.key,
        "url": found.url,
        "done": found.done,
        "hash": found.hash,
        "at": iso(found.at),
        "audience": String::from(found.audience),
        "external": known.output.is_some_and(|output| output.via.is_some()),
    })
}

#[cfg(test)]
mod tests {
    use domain::Language;
    use domain::workflow::StepRun;
    use orchestrator::StepSummary;
    use store::ConnectionRecord;

    use super::*;

    fn record(id: &str, provider: &str, capability: &str) -> ConnectionRecord {
        ConnectionRecord {
            id: id.into(),
            provider: provider.into(),
            label: provider.into(),
            account: None,
            capabilities: vec![capability.into()],
            settings: Value::Null,
        }
    }

    fn step(id: &str) -> StepSummary {
        StepSummary {
            id: id.into(),
            branch: None,
            kind: None,
            state: StepState::Passed,
            attempts: 1,
            seconds: 0,
            since: None,
        }
    }

    fn summary(steps: &[&str]) -> ItemSummary {
        ItemSummary {
            id: "i".into(),
            title: String::new(),
            workflow: "w".into(),
            parent: None,
            group: domain::board::Group::Delivered,
            status: None,
            issue: None,
            branch: String::new(),
            impediment: None,
            asks: 0,
            plans: 0,
            steps: steps.iter().map(|id| step(id)).collect(),
            current: None,
            live: false,
            ending: None,
            created_at: 0,
            updated_at: 0,
            ended_at: Some(1_700_000_000_000),
        }
    }

    fn finished(steps: Vec<StepRun>, delivered: Option<Delivered>) -> WorkItem {
        WorkItem { steps, ending: Some(Ending::Finished { output: Value::Null, delivered }), ..WorkItem::default() }
    }

    fn ran(id: &str, output: Value) -> StepRun {
        StepRun { id: id.into(), output, ..StepRun::default() }
    }

    fn known<'a>(connections: &'a [ConnectionRecord], output: Option<&'a Output>) -> Known<'a> {
        Known {
            connections,
            stale: &[],
            descriptors: &[],
            described: &Value::Null,
            output,
            language: Language::default(),
        }
    }

    #[test]
    fn an_item_that_delivers_a_report_shows_the_file_and_no_change() {
        let output = Output::read(&json!({ "kind": "report", "audience": "group:legal" })).ok().flatten();
        let connections = [record("c-code", "forge", "code")];
        let known = known(&connections, output.as_ref());
        let item =
            finished(vec![ran("write", json!({ "report": "parecer.pdf", "url": "/api/artifacts/a1/content" }))], None);
        let summary = summary(&["write"]);
        assert_eq!(change(&known, &summary, Some(&item)), Value::Null);
        let shown = delivered(&known, &summary, Some(&item));
        assert_eq!(shown["kind"], "report");
        assert_eq!(shown["key"], "parecer.pdf");
        assert_eq!(shown["url"], "/api/artifacts/a1/content");
        assert_eq!(shown["audience"], "group:legal");
        assert_eq!(shown["external"], false);
        assert_eq!(shown["hash"].as_str().map(str::len), Some(64));
    }

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
        let item = finished(steps, None);
        let summary = summary(&["open", "close"]);
        let shown = change(&known, &summary, Some(&item));
        assert_eq!(
            (shown["key"].clone(), shown["url"].clone(), shown["merged"].clone()),
            (json!("o/r#7"), json!("https://x/7"), json!(true))
        );
        assert_eq!(delivered(&known, &summary, Some(&item))["external"], true);
        let started = WorkItem {
            input: json!({ "sources": [{ "kind": "opened", "connection": "c-code", "subject": "o/r#3" }] }),
            ..WorkItem::default()
        };
        assert_eq!(change(&known, &summary, Some(&started))["key"], "o/r#3");
        assert_eq!(delivered(&known, &summary, Some(&started)), Value::Null);
    }

    #[test]
    fn the_artifact_kept_at_the_end_wins_and_a_workflow_without_output_shows_nothing() {
        let kept = Delivered {
            kind: "report".into(),
            key: "final.md".into(),
            url: None,
            done: true,
            hash: "ab".into(),
            at: 5,
            audience: domain::workflow::Audience::Me,
        };
        let item = finished(vec![ran("write", json!({ "report": "draft.md" }))], Some(kept));
        let output = Output::read(&json!({ "kind": "report" })).ok().flatten();
        let summary = summary(&["write"]);
        assert_eq!(delivered(&known(&[], output.as_ref()), &summary, Some(&item))["key"], "final.md");
        let plain = finished(vec![ran("write", json!({ "report": "draft.md" }))], None);
        assert_eq!(delivered(&known(&[], None), &summary, Some(&plain)), Value::Null);
        assert_eq!(change(&known(&[], None), &summary, Some(&plain)), Value::Null);
    }
}
