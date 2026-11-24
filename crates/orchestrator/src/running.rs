use crate::agent::WAITING_ANSWER;
use crate::api::Orchestrator;
use crate::error::Result;
use crate::ledger::Ledger;
use crate::protocol::{AGENT, kind_of};

impl Orchestrator {
    pub fn runs_session(&self, session: &str) -> Result<bool> {
        for item in self.shared.store.active_items()? {
            let stopped = item.impediment.as_ref().is_some_and(|found| found.reason.as_deref() != Some(WAITING_ANSWER));
            if stopped {
                continue;
            }
            let ledger = Ledger::load(&self.shared.store, &item.id)?;
            let ours = |seq: &u64| ledger.progress.get(seq).and_then(|data| data["session"].as_str()) == Some(session);
            if ledger.open().any(|request| kind_of(&request.effect) == AGENT && ours(&request.key.seq)) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
