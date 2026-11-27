use std::path::Path;

use agent_engine::sessions::{FileSessions, records};
use domain::SessionId;
use domain::catalog::Listed;
use domain::trace::{HistoryStamp, Record};

use crate::services::LocalServices;

pub(crate) fn listed(services: &LocalServices) -> Vec<Listed> {
    import_what_the_settings_listed(services);
    services.catalog.read().listed().to_vec()
}

pub(crate) fn history(services: &LocalServices, cwd: &Path, id: &SessionId) -> Result<Vec<Record>, String> {
    kept_history(&services.sessions, cwd, id)
}

pub(crate) fn history_since(
    services: &LocalServices,
    cwd: &Path,
    id: &SessionId,
    known: Option<HistoryStamp>,
) -> Result<Since, String> {
    kept_since(&services.sessions, cwd, id, known)
}

type Since = (Option<HistoryStamp>, Option<Vec<Record>>);

fn kept_since(store: &FileSessions, cwd: &Path, id: &SessionId, known: Option<HistoryStamp>) -> Result<Since, String> {
    let stamp = store.stamp(cwd, id)?;
    if stamp.is_some() && stamp == known {
        return Ok((stamp, None));
    }
    Ok((stamp, Some(kept_history(store, cwd, id)?)))
}

fn kept_history(store: &FileSessions, cwd: &Path, id: &SessionId) -> Result<Vec<Record>, String> {
    let told = records(store.entries(cwd, id)?);
    Ok(told
        .into_iter()
        .filter(|record| matches!(record, Record::Prompted { .. } | Record::Shown { .. } | Record::Event { .. }))
        .collect())
}

fn import_what_the_settings_listed(services: &LocalServices) {
    if !services.catalog.is_empty() {
        return;
    }
    let Some(owner_settings) = services.settings.load() else { return };
    services.catalog.update(&mut |catalog| catalog.import(&owner_settings));
}

#[cfg(test)]
mod tests;
