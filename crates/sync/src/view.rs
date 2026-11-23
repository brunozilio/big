use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

use async_trait::async_trait;
use store::Store;

use crate::desired::{Desired, desired_of};
use crate::error::Error;
use crate::policy::ReconcilePolicy;

#[async_trait]
pub trait ItemView: Send + Sync + std::fmt::Debug {
    async fn active(&self) -> Result<Vec<Desired>, Error>;
}

#[derive(Debug)]
pub struct StoreView {
    store: Arc<Store>,
    policies: RwLock<HashMap<String, ReconcilePolicy>>,
}

impl StoreView {
    pub fn new(store: Arc<Store>) -> StoreView {
        StoreView { store, policies: RwLock::default() }
    }

    pub fn set_policy(&self, workflow: &str, policy: ReconcilePolicy) {
        self.policies.write().unwrap_or_else(PoisonError::into_inner).insert(workflow.to_owned(), policy);
    }

    pub fn policy(&self, workflow: &str) -> ReconcilePolicy {
        self.policies.read().unwrap_or_else(PoisonError::into_inner).get(workflow).copied().unwrap_or_default()
    }
}

#[async_trait]
impl ItemView for StoreView {
    async fn active(&self) -> Result<Vec<Desired>, Error> {
        let items = self.store.items()?;
        Ok(items
            .iter()
            .filter(|item| !item.is_ended())
            .map(|item| desired_of(item, self.policy(&item.workflow)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use domain::workflow::{ItemId, WorkflowEvent};
    use serde_json::json;

    use super::*;
    use crate::policy::FieldPolicy;

    fn created(id: &str, workflow: &str) -> WorkflowEvent {
        WorkflowEvent::ItemCreated { id: ItemId::from(id), workflow: workflow.into(), parent: None, input: json!({}) }
    }

    #[tokio::test]
    async fn only_items_still_running_are_active_and_carry_their_workflow_policy() {
        let Ok(store) = Store::in_memory(0) else { panic!("store") };
        let store = Arc::new(store);
        assert!(store.append(&ItemId::from("a"), &[created("a", "w1")]).is_ok());
        let done = [created("b", "w2"), WorkflowEvent::Finished { output: json!(null) }];
        assert!(store.append(&ItemId::from("b"), &done).is_ok());
        let view = StoreView::new(store);
        view.set_policy("w1", ReconcilePolicy { status: FieldPolicy::Local, ..ReconcilePolicy::default() });
        let Ok(active) = view.active().await else { panic!("active") };
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].item, ItemId::from("a"));
        assert_eq!(active[0].policy.status, FieldPolicy::Local);
        assert_eq!(view.policy("unknown"), ReconcilePolicy::default());
    }
}
