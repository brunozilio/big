use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use agent_engine::RunHandle;
use domain::effect::IdempotencyKey;
use domain::team::TeammateAsk;
use domain::workflow::{Assignee, ItemId};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assign: Option<Assignee>,
}

struct OpenAsk {
    pending: PendingAsk,
    key: IdempotencyKey,
    run: Option<SharedRun>,
    teammate: Option<(u64, TeammateAsk)>,
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
                || (known.restored && known.key == key && supersedes(&known.pending.ask.kind, &pending.ask.kind))
        });
        *open = kept;
        open.push(OpenAsk { pending, key, run: Some(run), teammate: None, restored: false });
        replaced.into_iter().map(|gone| gone.pending).collect()
    }

    pub fn open_teammate(&self, pending: PendingAsk, key: IdempotencyKey, client: u64, asked: TeammateAsk) -> bool {
        let mut open = self.held();
        if open.iter().any(|known| known.pending.ask.id == pending.ask.id) {
            return false;
        }
        open.push(OpenAsk { pending, key, run: None, teammate: Some((client, asked)), restored: false });
        true
    }

    pub fn restore(&self, pending: PendingAsk, key: IdempotencyKey) {
        let mut open = self.held();
        if !open.iter().any(|known| known.pending.ask.id == pending.ask.id) {
            open.push(OpenAsk { pending, key, run: None, teammate: None, restored: true });
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

    pub fn asking(&self, item: &ItemId, step: &str) -> bool {
        self.held().iter().any(|known| &known.pending.item == item && known.pending.step == step)
    }

    pub fn plans(&self, item: &ItemId) -> usize {
        let planned = |known: &&OpenAsk| matches!(known.pending.ask.kind, domain::AskKind::Plan { .. });
        self.held().iter().filter(|known| &known.pending.item == item).filter(planned).count()
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
        let known = {
            let mut open = self.held();
            let at = open.iter().position(|known| &known.pending.ask.id == ask)?;
            open.remove(at)
        };
        let OpenAsk { pending, key, run, teammate, restored } = known;
        let kind = pending.ask.kind.clone();
        if let Some((client, asked)) = &teammate
            && let Err(error) = agent_engine::team::lead_inbox::answer(*client, asked, decision)
        {
            tracing::warn!(%error, "a teammate's question could not be answered");
        }
        if let Some(run) = &run {
            run.lock().unwrap_or_else(PoisonError::into_inner).answer(ask, decision);
        }
        if restored {
            let early = Early { key: key.clone(), kind, decision: decision.clone() };
            self.early.lock().unwrap_or_else(PoisonError::into_inner).push(early);
        }
        Some(Answered { pending, key })
    }

    pub fn take_early(&self, key: &IdempotencyKey, kind: &AskKind) -> Option<Decision> {
        let mut early = self.early.lock().unwrap_or_else(PoisonError::into_inner);
        let at = early.iter().position(|found| &found.key == key && same_choices(&found.kind, kind))?;
        Some(early.remove(at).decision)
    }
}

fn supersedes(old: &AskKind, new: &AskKind) -> bool {
    std::mem::discriminant(old) == std::mem::discriminant(new)
}

fn same_choices(old: &AskKind, new: &AskKind) -> bool {
    let labels = |question: &domain::Question| -> Vec<String> {
        question.options.iter().map(|choice| choice.label.clone()).collect()
    };
    match (old, new) {
        (AskKind::Questions(before), AskKind::Questions(after)) => {
            before.len() == after.len()
                && before.iter().zip(after).all(|(a, b)| a.multi == b.multi && labels(a) == labels(b))
        }
        _ => old == new,
    }
}

pub fn restore(shared: &crate::shared::Shared) -> crate::error::Result<()> {
    for item in shared.store.active_items()? {
        for entry in item.effects.values().filter(|entry| entry.outcome.is_none()) {
            let Some(open) = entry.progress["asks"].as_object() else { continue };
            for pending in open.values().filter_map(|value| serde_json::from_value::<PendingAsk>(value.clone()).ok()) {
                shared.asks.restore(pending, entry.request.key.clone());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use domain::{Choice, Question};

    use super::*;

    fn asked(description: &str, labels: &[&str]) -> AskKind {
        let options =
            labels.iter().map(|label| Choice { label: (*label).into(), description: description.into() }).collect();
        AskKind::Questions(vec![Question { header: "Escopo".into(), text: description.into(), options, multi: false }])
    }

    fn key() -> IdempotencyKey {
        IdempotencyKey { item: "i".into(), step: "plan".into(), branch: None, seq: 9, attempt: 0 }
    }

    fn restored(asks: &Asks, kind: AskKind) -> AskId {
        let id = AskId("ask-old".into());
        let pending = PendingAsk {
            item: ItemId::default(),
            step: "plan".into(),
            session: None,
            ask: AgentAsk { id: id.clone(), kind },
            since: 0,
            assign: None,
        };
        asks.restore(pending, key());
        id
    }

    #[test]
    fn an_early_answer_reaches_the_question_asked_again_in_other_words() {
        let asks = Asks::default();
        let id = restored(&asks, asked("devolver o valor", &["Simples", "Cadeia"]));
        let decision = Decision::Answer(vec![vec![0]]);
        assert!(asks.answer(&id, &decision).is_some());
        assert_eq!(asks.take_early(&key(), &asked("devolver valor", &["Simples", "Cadeia"])), Some(decision));
    }

    #[test]
    fn an_early_answer_is_not_given_to_a_question_with_other_choices() {
        let asks = Asks::default();
        let id = restored(&asks, asked("x", &["Simples", "Cadeia"]));
        assert!(asks.answer(&id, &Decision::Answer(vec![vec![0]])).is_some());
        assert_eq!(asks.take_early(&key(), &asked("x", &["Cadeia", "Simples"])), None);
    }

    struct EndsWhenAnswered(Arc<Asks>);

    impl RunHandle for EndsWhenAnswered {
        fn cancel(&mut self) {}

        fn answer(&mut self, _ask: &AskId, _decision: &Decision) {
            self.0.forget(&key());
        }
    }

    #[test]
    fn a_run_that_ends_the_moment_it_is_answered_still_counts_the_answer() {
        let asks = Arc::new(Asks::default());
        let id = AskId("ask-1".into());
        let pending = PendingAsk {
            item: ItemId::default(),
            step: "plan".into(),
            session: None,
            ask: AgentAsk { id: id.clone(), kind: AskKind::Plan { plan: "p".into() } },
            since: 0,
            assign: None,
        };
        let run: SharedRun = Arc::new(Mutex::new(Box::new(EndsWhenAnswered(Arc::clone(&asks)))));
        asks.open(pending, key(), run);
        let answered = asks.answer(&id, &Decision::Approve(domain::PermissionMode::Auto));
        assert_eq!(answered.map(|answered| answered.pending.ask.id), Some(id));
        assert!(asks.list().is_empty());
    }

    #[test]
    fn a_question_asked_again_supersedes_the_restored_one_but_not_a_plan() {
        let question = asked("a", &["Simples"]);
        assert!(supersedes(&question, &asked("b", &["Outra"])));
        assert!(!supersedes(&question, &AskKind::Plan { plan: "p".into() }));
    }
}
