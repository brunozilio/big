use crate::api::Orchestrator;
use crate::error::Result;
use crate::ledger::Ledger;
use crate::protocol::{AGENT, kind_of};

impl Orchestrator {
    pub fn steer(&self, session: &str, text: &str) -> bool {
        self.shared.steering.steer(session, text)
    }

    pub fn background_jobs(&self, session: &str) -> Option<Vec<(String, String, bool)>> {
        self.shared.steering.client(session).map(agent_engine::tools::clients::background)
    }

    pub fn stop_background(&self, session: &str, job: &str) -> Option<bool> {
        let client = self.shared.steering.client(session)?;
        Some(agent_engine::tools::clients::stop_background(client, job))
    }

    pub fn runs_session(&self, session: &str) -> Result<bool> {
        for item in self.shared.store.active_items()? {
            let stopped =
                item.impediment.as_ref().is_some_and(|found| !crate::texts::is_waiting_answer(found.reason.as_deref()));
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
