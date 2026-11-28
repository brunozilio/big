use domain::Language;
use domain::workflow::{Delivered, Ending, Output, StepRun, StepState, WorkItem};
use orchestrator::{ItemSummary, StepSummary};
use serde_json::Value;
use store::ConnectionRecord;

use crate::item_view::Known;

pub fn record(id: &str, provider: &str, capability: &str) -> ConnectionRecord {
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
    StepSummary { id: id.into(), branch: None, kind: None, state: StepState::Passed, attempts: 1, seconds: 0, since: None }
}

pub fn summary(steps: &[&str]) -> ItemSummary {
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

pub fn finished(steps: Vec<StepRun>, delivered: Option<Delivered>) -> WorkItem {
    WorkItem { steps, ending: Some(Ending::Finished { output: Value::Null, delivered }), ..WorkItem::default() }
}

pub fn ran(id: &str, output: Value) -> StepRun {
    StepRun { id: id.into(), output, ..StepRun::default() }
}

pub fn known<'a>(connections: &'a [ConnectionRecord], output: Option<&'a Output>) -> Known<'a> {
    Known { connections, stale: &[], descriptors: &[], described: &Value::Null, output, language: Language::default() }
}
