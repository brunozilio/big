mod cost;
mod view;

use std::sync::Arc;

use serde::Serialize;

pub use cost::{CostView, ItemCost, Spend, SpendLine, StepCost, cost_view};
pub use view::{Checkpoint, Done, ItemView, Next, StepView, Window, item_view, title_of};

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Ask {
    pub person: Option<String>,
    pub since_ms: i64,
    pub until_ms: i64,
    pub limit: usize,
}

impl Ask {
    pub const fn window(&self) -> Window {
        Window { since_ms: self.since_ms, until_ms: self.until_ms }
    }
}

pub trait Ledger: Send + Sync {
    fn items(&self, asker: &str, ask: &Ask) -> Result<Vec<ItemView>, String>;
    fn item(&self, asker: &str, id: &str, ask: &Ask) -> Result<ItemView, String>;
    fn cost(&self, asker: &str, ask: &Ask) -> Result<CostView, String>;
}

#[derive(Clone)]
pub struct HostLedger(pub Arc<dyn Ledger>);

impl std::fmt::Debug for HostLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostLedger")
    }
}

impl PartialEq for HostLedger {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[cfg(test)]
mod tests;
