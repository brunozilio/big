use domain::catalog::Listed;
use domain::models::usage_runs::RunUsage;
use serde_json::{Value, json};

use crate::usage::counted;

pub fn by_run(runs: &[RunUsage], sessions: &[Listed]) -> Value {
    let title = |session: &str| {
        sessions.iter().find(|listed| listed.session.0 == session).and_then(|listed| listed.title.clone())
    };
    let rows: Vec<Value> = runs
        .iter()
        .map(|run| {
            let subagents: Vec<Value> = run
                .subagents
                .iter()
                .map(|lane| {
                    counted(json!({ "index": lane.index, "kind": lane.kind, "summary": lane.summary }), &lane.tally)
                })
                .collect();
            let fields = json!({
                "id": run.session, "title": title(&run.session), "agent": run.agent,
                "item": run.item, "step": run.step,
                "lead": counted(json!({}), &run.lead), "subagents": subagents,
            });
            counted(fields, &run.tally)
        })
        .collect();
    json!(rows)
}

#[cfg(test)]
#[path = "usage_runs_tests.rs"]
mod tests;
