use std::path::PathBuf;
use std::sync::Arc;

use domain::workflow::{ItemId, WorkflowEvent};
use domain::{ChoreAnswer, SessionCommand};
use serde_json::Value;

use crate::shared::Shared;

async fn asked(shared: &Shared, text: String, cwd: PathBuf, wanted: Option<&str>) -> Option<String> {
    let chore = crate::chores::Chore {
        command: SessionCommand::Title,
        text,
        cwd,
        model: wanted.map(str::to_owned),
        resume: None,
    };
    match crate::chores::answer(shared, chore).await {
        Some(ChoreAnswer::Title(title)) => Some(title).filter(|title| !title.trim().is_empty()),
        _ => None,
    }
}

pub fn name(shared: &Arc<Shared>, item: ItemId, text: String, cwd: PathBuf, wanted: Option<String>) {
    let shared = Arc::clone(shared);
    let held = Arc::clone(&shared);
    held.spawn(async move {
        let Some(title) = asked(&shared, text, cwd, wanted.as_deref()).await else { return };
        if let Err(error) = shared.append(&item, &[WorkflowEvent::Named { title }]) {
            tracing::warn!(%error, "the item's title could not be kept");
        }
    });
}

pub fn titled(input: &Value) -> bool {
    let issue = input["issue"].as_str().is_some_and(|issue| !issue.is_empty());
    let sources = input["sources"].as_array().map(Vec::as_slice).unwrap_or_default();
    issue || sources.iter().any(|source| matches!(source["kind"].as_str(), Some("issue" | "pull_request")))
}
