use std::path::PathBuf;

use domain::workflow::WorkItem;

use crate::chores::{model, runs, wanted_model};
use crate::shared::Shared;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSession {
    pub session: String,
    pub cwd: PathBuf,
}

pub fn sessions(item: &WorkItem) -> Vec<ItemSession> {
    let mut found: Vec<ItemSession> = Vec::new();
    for ran in runs(item) {
        found.retain(|known| known.session != ran.session);
        found.push(ItemSession { session: ran.session, cwd: ran.cwd });
    }
    found
}

pub fn window(shared: &Shared, item: &WorkItem, used_model: Option<&str>) -> u64 {
    let wanted = used_model.map(str::to_owned).or_else(|| wanted_model(item));
    agent_engine::upkeep::window(&model(shared, wanted.as_deref()))
}
