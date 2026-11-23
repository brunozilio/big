use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use agent_engine::RunHandle;
use domain::effect::IdempotencyKey;
use domain::workflow::ItemId;
use domain::{AgentAsk, AskId, AskKind, Decision};
use serde::{Deserialize, Serialize};

pub type SharedRun = Arc<Mutex<Box<dyn RunHandle>>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    run: Option<SharedRun>,
    restored: bool,
}

struct Early {
    key: IdempotencyKey,
    kind: AskKind,
    decision: Decision,
}

#[derive(Default)]
pub struct Asks {
    open: Mutex<Vec<OpenAsk>>,
    early: Mutex<Vec<Early>>,
}

impl fmt::Debug for Asks {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Asks").field("open", &self.list().len()).finish()
    }
}

pub struct Answered {
    pub pending: PendingAsk,
    pub key: IdempotencyKey,
}

impl Asks {
    fn held(&self) -> std::sync::MutexGuard<'_, Vec<OpenAsk>> {
        self.open.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn open(&self, pending: PendingAsk, key: IdempotencyKey, run: SharedRun) -> Vec<PendingAsk> {
        let mut open = self.held();
        let (replaced, kept): (Vec<OpenAsk>, Vec<OpenAsk>) = open.drain(..).partition(|known| {
            known.pending.ask.id == pending.ask.id
                || (known.restored && known.key == key && known.pending.ask.kind == pending.ask.kind)
        });
        *open = kept;
        open.push(OpenAsk { pending, key, run: Some(run), restored: false });
        replaced.into_iter().map(|gone| gone.pending).collect()
    }

    pub fn restore(&self, pending: PendingAsk, key: IdempotencyKey) {
        let mut open = self.held();
        if !open.iter().any(|known| known.pending.ask.id == pending.ask.id) {
            open.push(OpenAsk { pending, key, run: None, restored: true });
        }
    }

    pub fn attach(&self, key: &IdempotencyKey, run: &SharedRun) -> Vec<PendingAsk> {
        let mut open = self.held();
        let restored = open.iter_mut().filter(|known| &known.key == key && known.run.is_none());
        restored
            .map(|known| {
                known.run = Some(Arc::clone(run));
                known.pending.clone()
            })
            .collect()
    }

    pub fn close(&self, ask: &AskId) -> Option<PendingAsk> {
        let mut open = self.held();
        let at = open.iter().position(|known| &known.pending.ask.id == ask)?;
        Some(open.remove(at).pending)
    }

    pub fn list(&self) -> Vec<PendingAsk> {
        self.held().iter().map(|known| known.pending.clone()).collect()
    }

    pub fn count(&self, item: &ItemId) -> usize {
        self.held().iter().filter(|known| &known.pending.item == item).count()
    }

    pub fn of_key(&self, key: &IdempotencyKey) -> Vec<PendingAsk> {
        self.held().iter().filter(|known| &known.key == key).map(|known| known.pending.clone()).collect()
    }

    pub fn forget(&self, key: &IdempotencyKey) -> Vec<PendingAsk> {
        let mut open = self.held();
        let (gone, kept): (Vec<OpenAsk>, Vec<OpenAsk>) = open.drain(..).partition(|known| &known.key == key);
        *open = kept;
        self.early.lock().unwrap_or_else(PoisonError::into_inner).retain(|early| &early.key != key);
        gone.into_iter().map(|known| known.pending).collect()
    }

    pub fn answer(&self, ask: &AskId, decision: &Decision) -> Option<Answered> {
        let (run, key, kind, restored) = {
            let open = self.held();
            let known = open.iter().find(|known| &known.pending.ask.id == ask)?;
            (known.run.clone(), known.key.clone(), known.pending.ask.kind.clone(), known.restored)
        };
        if let Some(run) = &run {
            run.lock().unwrap_or_else(PoisonError::into_inner).answer(ask, decision);
        }
        if restored {
            let early = Early { key: key.clone(), kind, decision: decision.clone() };
            self.early.lock().unwrap_or_else(PoisonError::into_inner).push(early);
        }
        self.close(ask).map(|pending| Answered { pending, key })
    }

    pub fn take_early(&self, key: &IdempotencyKey, kind: &AskKind) -> Option<Decision> {
        let mut early = self.early.lock().unwrap_or_else(PoisonError::into_inner);
        let at = early.iter().position(|found| &found.key == key && &found.kind == kind)?;
        Some(early.remove(at).decision)
    }
}

pub fn restore(shared: &crate::shared::Shared) -> crate::error::Result<()> {
    for item in shared.store.items()?.into_iter().filter(|item| !item.is_ended()) {
        for entry in item.effects.values().filter(|entry| entry.outcome.is_none()) {
            let Some(open) = entry.progress["asks"].as_object() else { continue };
            for pending in open.values().filter_map(|value| serde_json::from_value::<PendingAsk>(value.clone()).ok()) {
                shared.asks.restore(pending, entry.request.key.clone());
            }
        }
    }
    Ok(())
}
