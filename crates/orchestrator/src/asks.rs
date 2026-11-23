use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use agent_engine::RunHandle;
use domain::effect::IdempotencyKey;
use domain::workflow::ItemId;
use domain::{AgentAsk, AskId, Decision};
use serde::Serialize;

pub type SharedRun = Arc<Mutex<Box<dyn RunHandle>>>;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingAsk {
    pub item: ItemId,
    pub step: String,
    pub session: Option<String>,
    pub ask: AgentAsk,
    pub since: i64,
}

struct OpenAsk {
    pending: PendingAsk,
    key: IdempotencyKey,
    run: SharedRun,
}

#[derive(Default)]
pub struct Asks {
    open: Mutex<Vec<OpenAsk>>,
}

impl fmt::Debug for Asks {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Asks").field("open", &self.list().len()).finish()
    }
}

impl Asks {
    pub fn open(&self, pending: PendingAsk, key: IdempotencyKey, run: SharedRun) {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        open.retain(|known| known.pending.ask.id != pending.ask.id);
        open.push(OpenAsk { pending, key, run });
    }

    pub fn close(&self, ask: &AskId) -> Option<PendingAsk> {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let at = open.iter().position(|known| &known.pending.ask.id == ask)?;
        Some(open.remove(at).pending)
    }

    pub fn list(&self) -> Vec<PendingAsk> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner).iter().map(|known| known.pending.clone()).collect()
    }

    pub fn count(&self, item: &ItemId) -> usize {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|known| &known.pending.item == item)
            .count()
    }

    pub fn of_key(&self, key: &IdempotencyKey) -> Vec<PendingAsk> {
        let open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        open.iter().filter(|known| &known.key == key).map(|known| known.pending.clone()).collect()
    }

    pub fn forget(&self, key: &IdempotencyKey) -> Vec<PendingAsk> {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let (gone, kept): (Vec<OpenAsk>, Vec<OpenAsk>) = open.drain(..).partition(|known| &known.key == key);
        *open = kept;
        gone.into_iter().map(|known| known.pending).collect()
    }

    pub fn answer(&self, ask: &AskId, decision: &Decision) -> Option<PendingAsk> {
        let run = {
            let open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
            open.iter().find(|known| &known.pending.ask.id == ask).map(|known| Arc::clone(&known.run))?
        };
        run.lock().unwrap_or_else(PoisonError::into_inner).answer(ask, decision);
        self.close(ask)
    }
}
