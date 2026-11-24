use domain::{AskKind, Decision, EngineEvent, PermissionMode};
use serde_json::{Value, json};

use crate::agent::Agent;

pub(in crate::agent) const HANDED_OFF: &str =
    "The user approved your plan and planning is over. Another agent carries it out, so do not implement anything.";

impl Agent<'_> {
    pub(in crate::agent) fn put_plan(&self, id: &str, plan: &str) -> Value {
        self.notify("plan_approval", concat!(brand::name!(), "'s plan is ready for your review"));
        match self.asked(id, AskKind::Plan { plan: plan.to_owned() }) {
            Ok(Decision::Approve(PermissionMode::Plan)) => json!({ "handedOff": HANDED_OFF }),
            Ok(Decision::Approve(mode)) => {
                self.control.set_mode(mode);
                let mode = self.control.mode();
                (self.emit)(EngineEvent::ModeChanged(mode));
                json!({ "approved": format!("{mode:?}").to_ascii_lowercase() })
            }
            Ok(Decision::DenyWith(note)) => json!({ "kept": true, "note": note }),
            Ok(Decision::Deny | Decision::Allow | Decision::AllowAlways | Decision::Answer(_)) => {
                json!({ "kept": true })
            }
            Err(_) => json!({ "stopped": true }),
        }
    }
}
