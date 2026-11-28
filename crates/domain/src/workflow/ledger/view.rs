use serde::Serialize;
use serde_json::Value;

use super::cost::{Spend, SpendLine};
use crate::board::{Group, group_of};
use crate::workflow::{ItemId, StepState, WorkItem, WorkflowEvent};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Window {
    pub since_ms: i64,
    pub until_ms: i64,
}

impl Window {
    pub const fn holds(self, at: i64) -> bool {
        at >= self.since_ms && at < self.until_ms
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepView {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub state: StepState,
    pub attempt: u32,
    pub started_ms: Option<i64>,
    pub finished_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Done {
    Step { step: String, branch: Option<String>, state: StepState, at_ms: i64 },
    Answered { step: String, action: String, at_ms: i64 },
    Status { status: String, at_ms: i64 },
    Finished { at_ms: i64 },
    Cancelled { reason: String, at_ms: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Next {
    Running { step: String, branch: Option<String> },
    WaitingHuman { step: String, branch: Option<String> },
    Pending { step: String, branch: Option<String> },
    Impediment { reason: Option<String> },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Checkpoint {
    pub work_done: Vec<Done>,
    pub next_steps: Vec<Next>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemView {
    pub id: ItemId,
    pub title: String,
    pub workflow: String,
    pub owner: String,
    pub parent: Option<ItemId>,
    pub group: Group,
    pub status: Option<String>,
    pub ended: bool,
    pub created_ms: Option<i64>,
    pub updated_ms: Option<i64>,
    pub recap: Option<String>,
    pub steps: Vec<StepView>,
    pub checkpoint: Checkpoint,
    pub cost: Spend,
}

fn text_in<'a>(input: &'a Value, name: &str) -> Option<&'a str> {
    input.get(name).and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty())
}

pub fn title_of(item: &WorkItem) -> String {
    let named = item.name.as_deref().map(str::trim).filter(|name| !name.is_empty());
    let first_line = text_in(&item.input, "text").and_then(|text| text.lines().next());
    named.or_else(|| text_in(&item.input, "title")).or(first_line).unwrap_or(&item.workflow).to_owned()
}

fn done_of(event: &WorkflowEvent, at_ms: i64) -> Option<Done> {
    match event {
        WorkflowEvent::StepFinished { step, branch, state, .. } => {
            Some(Done::Step { step: step.clone(), branch: branch.clone(), state: *state, at_ms })
        }
        WorkflowEvent::HumanInput { step, action, .. } => {
            Some(Done::Answered { step: step.clone(), action: action.clone(), at_ms })
        }
        WorkflowEvent::StatusSet(status) => Some(Done::Status { status: status.clone(), at_ms }),
        WorkflowEvent::Finished { .. } => Some(Done::Finished { at_ms }),
        WorkflowEvent::Cancelled { reason } => Some(Done::Cancelled { reason: reason.clone(), at_ms }),
        _ => None,
    }
}

fn next_steps(item: &WorkItem) -> Vec<Next> {
    if item.is_ended() {
        return Vec::new();
    }
    let open = item.steps.iter().filter_map(|run| {
        let (step, branch) = (run.id.clone(), run.branch.clone());
        match run.state {
            StepState::Running => Some(Next::Running { step, branch }),
            StepState::WaitingHuman => Some(Next::WaitingHuman { step, branch }),
            StepState::Pending => Some(Next::Pending { step, branch }),
            StepState::Passed | StepState::Failed | StepState::Skipped => None,
        }
    });
    let stuck = item.impediment.as_ref().map(|impediment| Next::Impediment { reason: impediment.reason.clone() });
    open.chain(stuck).collect()
}

fn timed(item: &WorkItem, events: &[(u64, i64, WorkflowEvent)]) -> Vec<StepView> {
    let moment = |wanted: &dyn Fn(&WorkflowEvent) -> bool| {
        events.iter().rev().find(|(_, _, event)| wanted(event)).map(|(_, at, _)| *at)
    };
    item.steps
        .iter()
        .map(|run| {
            let started = moment(&|event| {
                matches!(event, WorkflowEvent::StepStarted { step, branch } if run.is(step, branch.as_deref()))
            });
            let finished = moment(&|event| {
                matches!(event, WorkflowEvent::StepFinished { step, branch, .. } if run.is(step, branch.as_deref()))
            });
            StepView {
                id: run.id.clone(),
                branch: run.branch.clone(),
                state: run.state,
                attempt: run.attempt,
                started_ms: started,
                finished_ms: finished.filter(|_| !matches!(run.state, StepState::Running | StepState::Pending)),
            }
        })
        .collect()
}

pub fn item_view(
    item: &WorkItem,
    owner: &str,
    events: &[(u64, i64, WorkflowEvent)],
    window: Window,
    spend: &[SpendLine],
) -> ItemView {
    let work_done =
        events.iter().filter(|(_, at, _)| window.holds(*at)).filter_map(|(_, at, event)| done_of(event, *at)).collect();
    ItemView {
        id: item.id.clone(),
        title: title_of(item),
        workflow: item.workflow.clone(),
        owner: owner.to_owned(),
        parent: item.parent.clone(),
        group: group_of(item, false),
        status: item.status.clone(),
        ended: item.is_ended(),
        created_ms: events.first().map(|(_, at, _)| *at),
        updated_ms: events.last().map(|(_, at, _)| *at),
        recap: item.recap.as_ref().map(|recap| recap.text.clone()),
        steps: timed(item, events),
        checkpoint: Checkpoint { work_done, next_steps: next_steps(item) },
        cost: Spend::of(spend.iter().filter(|line| line.item == item.id.as_str())),
    }
}

impl ItemView {
    pub fn redacted(mut self, clean: &dyn Fn(&str) -> String) -> Self {
        self.title = clean(&self.title);
        self.recap = self.recap.as_deref().map(clean);
        self.status = self.status.as_deref().map(clean);
        for done in &mut self.checkpoint.work_done {
            match done {
                Done::Status { status: text, .. } | Done::Cancelled { reason: text, .. } => *text = clean(text),
                Done::Step { .. } | Done::Answered { .. } | Done::Finished { .. } => {}
            }
        }
        for next in &mut self.checkpoint.next_steps {
            if let Next::Impediment { reason: Some(reason) } = next {
                *reason = clean(reason);
            }
        }
        self
    }
}
