use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::workflow::{ItemId, WorkflowEvent};

pub const STEP_STARTED: &str = "step.started";
pub const STEP_FINISHED: &str = "step.finished";
pub const HUMAN: &str = "human";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    Failure,
    Escape,
}

impl FindingKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Failure => "failure",
            Self::Escape => "escape",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub item: ItemId,
    pub workflow: String,
    pub generation: Option<String>,
    pub kind: FindingKind,
    pub step: String,
    pub gate: Option<String>,
    pub caught_by: Option<String>,
    pub detail: Value,
    pub at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin<'a> {
    pub item: &'a ItemId,
    pub workflow: &'a str,
    pub generation: Option<&'a str>,
}

#[derive(Clone, Debug, Default)]
struct Started {
    kind: String,
    options: Value,
}

#[derive(Clone, Debug)]
struct Passed {
    step: String,
    attempt: u32,
    order: usize,
    data: Value,
}

#[derive(Debug, Default)]
struct Scan {
    started: BTreeMap<String, Started>,
    passed: Vec<Passed>,
    found: Vec<Finding>,
    order: usize,
}

pub fn findings(origin: &Origin<'_>, gates: &[&str], events: &[(i64, WorkflowEvent)]) -> Vec<Finding> {
    let mut scan = Scan::default();
    for (at, event) in events {
        scan.order += 1;
        match event {
            WorkflowEvent::Signal { kind, step: Some(step), data, .. } if kind == STEP_STARTED => {
                let kind = data["type"].as_str().unwrap_or(step).to_owned();
                scan.started.insert(step.clone(), Started { kind, options: data["options"].clone() });
            }
            WorkflowEvent::Signal { kind, step: Some(step), attempt, data, .. } if kind == STEP_FINISHED => {
                scan.finished(origin, gates, step, *attempt, data, *at);
            }
            WorkflowEvent::Rewound { to } => {
                let caught = Caught { by: HUMAN, attempt: 0, data: &Value::Null, at: *at };
                scan.escapes(origin, gates, to, &caught);
            }
            _ => {}
        }
    }
    scan.found
}

struct Caught<'a> {
    by: &'a str,
    attempt: u32,
    data: &'a Value,
    at: i64,
}

impl Scan {
    fn finished(&mut self, origin: &Origin<'_>, gates: &[&str], step: &str, attempt: u32, data: &Value, at: i64) {
        let outcome = data["outcome"].as_str().unwrap_or_default();
        let result = &data["data"];
        if outcome == "passed" {
            let order = self.order;
            self.passed.push(Passed { step: step.to_owned(), attempt, order, data: result.clone() });
            return;
        }
        let back = match outcome {
            "failed" => self.started.get(step).and_then(|run| run.options["onFail"].as_str()).map(str::to_owned),
            "goto" => data["to"].as_str().map(str::to_owned),
            _ => return,
        };
        let id = format!("{}:{step}:{attempt}:failure", origin.item);
        let detail = json!({ "outcome": outcome, "to": back, "type": self.kind_of(step), "data": result });
        self.found.push(finding(origin, id, FindingKind::Failure, step, None, None, detail, at));
        if let Some(back) = back {
            let caught = Caught { by: step, attempt, data: result, at };
            self.escapes(origin, gates, &back, &caught);
        }
    }

    fn escapes(&mut self, origin: &Origin<'_>, gates: &[&str], producer: &str, caught: &Caught<'_>) {
        let Some(made) = self.passed.iter().rev().find(|run| run.step == producer).cloned() else { return };
        let let_through: Vec<Passed> = self
            .passed
            .iter()
            .filter(|run| run.order > made.order && run.step != caught.by && run.step != producer)
            .filter(|run| gates.contains(&self.kind_of(&run.step)))
            .cloned()
            .collect();
        for gate in let_through {
            let id = format!("{}:{}:{}:escape:{}:{}", origin.item, gate.step, gate.attempt, caught.by, caught.attempt);
            let detail = json!({
                "gate_type": self.kind_of(&gate.step),
                "gate_options": self.started.get(&gate.step).map_or(Value::Null, |run| run.options.clone()),
                "gate_output": gate.data,
                "producer_output": made.data,
                "caught": caught.data,
            });
            let kind = FindingKind::Escape;
            let found = finding(origin, id, kind, producer, Some(gate.step), Some(caught.by), detail, caught.at);
            self.found.push(found);
        }
    }

    fn kind_of<'s>(&'s self, step: &'s str) -> &'s str {
        self.started.get(step).map_or(step, |run| run.kind.as_str())
    }
}

#[allow(clippy::too_many_arguments)]
fn finding(
    origin: &Origin<'_>,
    id: String,
    kind: FindingKind,
    step: &str,
    gate: Option<String>,
    caught_by: Option<&str>,
    detail: Value,
    at: i64,
) -> Finding {
    Finding {
        id,
        item: origin.item.clone(),
        workflow: origin.workflow.to_owned(),
        generation: origin.generation.map(str::to_owned),
        kind,
        step: step.to_owned(),
        gate,
        caught_by: caught_by.map(str::to_owned),
        detail,
        at,
    }
}

pub fn generation<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut sorted: Vec<(&str, &str)> = files.into_iter().collect();
    sorted.sort_unstable();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (path, text) in sorted {
        for byte in path.bytes().chain([0]).chain(text.bytes()).chain([0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

pub const fn crossed(count: u64, threshold: u64) -> bool {
    threshold > 0 && count > 0 && count.is_multiple_of(threshold)
}

#[cfg(test)]
mod tests;
