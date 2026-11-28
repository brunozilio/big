use domain::workflow::{Delivered, Ending, WorkItem};
use orchestrator::ItemSummary;
use serde_json::{Value, json};

use crate::item_view::Known;
use crate::marks::iso;

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
    use domain::workflow::Output;
    use serde_json::json;

    use super::*;
    use crate::view_fixtures::{finished, known, ran, record, summary};

    #[test]
    fn an_item_that_delivers_a_report_shows_the_file() {
        let output = Output::read(&json!({ "kind": "report", "audience": "group:legal" })).ok().flatten();
        let connections = [record("c-code", "forge", "code")];
        let item =
            finished(vec![ran("write", json!({ "report": "parecer.pdf", "url": "/api/artifacts/a1/content" }))], None);
        let shown = delivered(&known(&connections, output.as_ref()), &summary(&["write"]), Some(&item));
        assert_eq!(shown["kind"], "report");
        assert_eq!(shown["key"], "parecer.pdf");
        assert_eq!(shown["url"], "/api/artifacts/a1/content");
        assert_eq!(shown["audience"], "group:legal");
        assert_eq!(shown["external"], false);
        assert_eq!(shown["hash"].as_str().map(str::len), Some(64));
    }

    #[test]
    fn an_artifact_kept_in_a_connection_is_marked_external_and_an_unfinished_item_delivers_nothing() {
        let output = Output::read(&json!({ "kind": "change", "key": "ref", "via": "code" })).ok().flatten();
        let known = known(&[], output.as_ref());
        let item = finished(vec![ran("open", json!({ "ref": "o/r#7" }))], None);
        assert_eq!(delivered(&known, &summary(&["open"]), Some(&item))["external"], true);
        let running = WorkItem { steps: item.steps.clone(), ..WorkItem::default() };
        assert_eq!(delivered(&known, &summary(&["open"]), Some(&running)), Value::Null);
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
    }
}
