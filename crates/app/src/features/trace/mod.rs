use std::path::Path;

use agent_engine::sessions::{FileSessions, records};
use domain::SessionId;
use domain::trace::runs::{Run, runs};

pub mod export;

pub(crate) fn traced(store: &FileSessions, cwd: &Path, id: &SessionId) -> Result<Vec<Run>, String> {
    let records = records(store.entries(cwd, id)?);
    if records.is_empty() {
        return Err("this session has nothing to trace yet".into());
    }
    Ok(runs(&id.0, &domain::trace::build(records)))
}

#[cfg(test)]
mod tests;
