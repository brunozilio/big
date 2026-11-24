use std::path::PathBuf;
use std::sync::Arc;

use domain::workflow::{ItemId, WorkflowEvent};
use serde_json::Value;

use crate::agent::rewind_session;
use crate::error::{Error, Result};
use crate::events::Wake;
use crate::git;
use crate::ledger::Ledger;
use crate::protocol::{AGENT, kind_of};
use crate::runner::ensure;
use crate::shared::Shared;

pub async fn rewind(shared: &Arc<Shared>, id: &ItemId, step: &str) -> Result<()> {
    let ledger = Ledger::load(&shared.store, id)?;
    let cut = ledger.cut(step).ok_or_else(|| Error::UnknownStep(step.to_owned()))?;
    let first = ledger
        .requested
        .range(cut..)
        .filter(|(_, request)| kind_of(&request.effect) == AGENT)
        .find_map(|(seq, _)| ledger.value(*seq).cloned());
    if let Some(agent) = first {
        if let Err(error) = crate::agent::worktrees(shared, id, None).await {
            tracing::warn!(?error, item = %id, "the item's worktrees could not be made again");
        }
        restore(shared, &agent).await;
    }
    shared.append(id, &[WorkflowEvent::Rewound { to: step.to_owned() }])?;
    ensure(shared, id);
    shared.wake(id, Wake::Rerun);
    Ok(())
}

async fn restore(shared: &Shared, agent: &Value) {
    let Some(worktree) = agent["worktree"].as_str().map(PathBuf::from) else { return };
    let mut places: Vec<(PathBuf, String)> = agent["repositories"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|repo| Some((PathBuf::from(repo["worktree"].as_str()?), repo["base"].as_str()?.to_owned())))
        .collect();
    if places.is_empty()
        && let Some(base) = agent["base"].as_str()
    {
        places.push((worktree.clone(), base.to_owned()));
    }
    for (place, base) in places {
        let _ = tokio::task::spawn_blocking(move || git::restore(&place, &base)).await;
    }
    let session = agent["session"].as_str();
    if let (Some(session), Some(checkpoint)) = (session, agent["checkpoint"].as_u64()) {
        let checkpoint = u32::try_from(checkpoint).unwrap_or(u32::MAX);
        rewind_session(shared, worktree, session, checkpoint).await;
    }
}
