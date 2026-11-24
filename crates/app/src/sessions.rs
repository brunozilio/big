use std::path::Path;

use agent_engine::sessions::{FileSessions, records};
use domain::SessionId;
use domain::catalog::Listed;
use domain::trace::Record;

use crate::services::LocalServices;

pub(crate) fn listed(services: &LocalServices) -> Vec<Listed> {
    import_what_the_settings_listed(services);
    services.catalog.read().listed().to_vec()
}

pub(crate) fn history(services: &LocalServices, cwd: &Path, id: &SessionId) -> Result<Vec<Record>, String> {
    kept_history(&services.sessions, cwd, id)
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
