use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldPolicy {
    Local,
    Remote,
    Event,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReconcilePolicy {
    pub status: FieldPolicy,
    pub impediment: FieldPolicy,
    pub pull_request: FieldPolicy,
    pub pipeline: FieldPolicy,
}

impl Default for ReconcilePolicy {
    fn default() -> ReconcilePolicy {
        ReconcilePolicy {
            status: FieldPolicy::Event,
            impediment: FieldPolicy::Local,
            pull_request: FieldPolicy::Event,
            pipeline: FieldPolicy::Event,
        }
    }
}

impl ReconcilePolicy {
    pub fn from_workflow(reconcile: &serde_json::Value) -> ReconcilePolicy {
        serde_json::from_value(reconcile.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_workflow_overrides_only_the_fields_it_names() {
        let policy = ReconcilePolicy::from_workflow(&json!({"status": "remote", "custom": "local"}));
        assert_eq!(policy.status, FieldPolicy::Remote);
        assert_eq!(policy.impediment, FieldPolicy::Local);
        assert_eq!(policy.pull_request, FieldPolicy::Event);
        assert_eq!(policy.pipeline, FieldPolicy::Event);
    }

    #[test]
    fn nothing_or_garbage_is_the_default() {
        assert_eq!(ReconcilePolicy::from_workflow(&json!(null)), ReconcilePolicy::default());
        assert_eq!(ReconcilePolicy::from_workflow(&json!({})), ReconcilePolicy::default());
        assert_eq!(ReconcilePolicy::from_workflow(&json!({"status": "nope"})), ReconcilePolicy::default());
    }
}
