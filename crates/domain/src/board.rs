use serde::{Deserialize, Serialize};

use crate::workflow::WorkItem;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    WithYou,
    InProgress,
    Delivered,
}

pub fn group_of(item: &WorkItem, pending_asks: bool) -> Group {
    if item.is_ended() {
        Group::Delivered
    } else if pending_asks || item.waiting_human() || item.impediment.is_some() {
        Group::WithYou
    } else {
        Group::InProgress
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::workflow::{Ending, Impediment, StepRun, StepState};

    fn with_step(state: StepState) -> WorkItem {
        WorkItem { steps: vec![StepRun { id: "anything".into(), state, ..StepRun::default() }], ..WorkItem::default() }
    }

    #[test]
    fn a_running_item_is_in_progress() {
        assert_eq!(group_of(&with_step(StepState::Running), false), Group::InProgress);
        assert_eq!(group_of(&WorkItem::default(), false), Group::InProgress);
    }

    #[test]
    fn a_step_waiting_for_a_human_puts_it_with_you() {
        assert_eq!(group_of(&with_step(StepState::WaitingHuman), false), Group::WithYou);
    }

    #[test]
    fn an_unanswered_ask_puts_it_with_you() {
        assert_eq!(group_of(&with_step(StepState::Running), true), Group::WithYou);
    }

    #[test]
    fn an_impediment_puts_it_with_you() {
        let item = WorkItem { impediment: Some(Impediment { reason: None }), ..with_step(StepState::Failed) };
        assert_eq!(group_of(&item, false), Group::WithYou);
    }

    #[test]
    fn an_ended_workflow_is_delivered_whatever_else_lingers() {
        let finished = WorkItem {
            ending: Some(Ending::Finished { output: json!(null), delivered: None }),
            impediment: Some(Impediment { reason: None }),
            ..with_step(StepState::WaitingHuman)
        };
        assert_eq!(group_of(&finished, true), Group::Delivered);
        let cancelled =
            WorkItem { ending: Some(Ending::Cancelled { reason: "x".into() }), ..with_step(StepState::Running) };
        assert_eq!(group_of(&cancelled, false), Group::Delivered);
    }

    #[test]
    fn the_status_name_never_decides_the_group() {
        let item = WorkItem { status: Some("Done".into()), ..with_step(StepState::Running) };
        assert_eq!(group_of(&item, false), Group::InProgress);
    }
}
