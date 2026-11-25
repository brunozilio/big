use std::collections::BTreeMap;

use domain::effect::{EffectOutcome, EffectRequest, IdempotencyKey};
use domain::workflow::{ItemId, WorkflowEvent};
use serde_json::Value;
use store::Store;

use crate::protocol::{journal_entry, kind_of};

#[derive(Debug, Clone, PartialEq)]
pub struct SignalRecord {
    pub kind: String,
    pub branch: String,
    pub step: Option<String>,
    pub attempt: u32,
    pub data: Value,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    pub outcome: EffectOutcome,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pause {
    pub seq: u64,
    pub ask: String,
    pub from: i64,
    pub to: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ledger {
    pub start: i64,
    pub requested: BTreeMap<u64, EffectRequest>,
    pub resolved: BTreeMap<u64, Resolution>,
    pub order: Vec<u64>,
    pub progress: BTreeMap<u64, Value>,
    pub signals: BTreeMap<u64, SignalRecord>,
    pub requested_at: BTreeMap<u64, i64>,
    pub pauses: Vec<Pause>,
    pub rewinds: u32,
}

impl Ledger {
    pub fn load(store: &Store, item: &ItemId) -> store::Result<Ledger> {
        let mut ledger = Ledger::default();
        for (_, at, event) in store.timed_events(item, 0)? {
            ledger.apply(&event, at);
        }
        Ok(ledger)
    }

    fn apply(&mut self, event: &WorkflowEvent, at: i64) {
        match event {
            WorkflowEvent::ItemCreated { .. } => self.start = at,
            WorkflowEvent::Signal { seq, kind, branch, step, attempt, data } => {
                let record = SignalRecord {
                    kind: kind.clone(),
                    branch: branch.clone(),
                    step: step.clone(),
                    attempt: *attempt,
                    data: data.clone(),
                    at,
                };
                self.signals.insert(*seq, record);
            }
            WorkflowEvent::EffectRequested(request) => {
                self.requested.insert(request.key.seq, request.clone());
                self.requested_at.insert(request.key.seq, at);
            }
            WorkflowEvent::EffectResolved { key, outcome } => self.resolve(key, outcome, at),
            WorkflowEvent::EffectProgress { key, data } if self.current(key) => {
                self.pause(key.seq, &data["asks"], at);
                domain::workflow::merge_patch(self.progress.entry(key.seq).or_insert(Value::Null), data);
            }
            WorkflowEvent::Rewound { to, at } => self.rewind(to, *at),
            _ => {}
        }
    }

    fn current(&self, key: &IdempotencyKey) -> bool {
        self.requested.get(&key.seq).is_some_and(|request| &request.key == key)
    }

    fn pause(&mut self, seq: u64, asks: &Value, at: i64) {
        let open = |pause: &&mut Pause| pause.seq == seq && pause.to.is_none();
        match asks {
            Value::Null => self.pauses.iter_mut().filter(open).for_each(|pause| pause.to = Some(at)),
            Value::Object(changed) => {
                for (ask, value) in changed {
                    let known = self.pauses.iter_mut().filter(open).find(|pause| &pause.ask == ask);
                    match (known, value.is_null()) {
                        (Some(pause), true) => pause.to = Some(at),
                        (None, false) => self.pauses.push(Pause { seq, ask: ask.clone(), from: at, to: None }),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn resolve(&mut self, key: &IdempotencyKey, outcome: &EffectOutcome, at: i64) {
        if !self.current(key) || self.resolved.contains_key(&key.seq) {
            return;
        }
        self.resolved.insert(key.seq, Resolution { outcome: outcome.clone(), at });
        self.order.push(key.seq);
    }

    pub fn cut(&self, step: &str) -> Option<u64> {
        self.signals
            .iter()
            .rev()
            .find(|(_, signal)| signal.kind == "step.started" && signal.step.as_deref() == Some(step))
            .map(|(seq, _)| *seq)
    }

    pub fn cut_before(&self, step: &str, seq: u64) -> Option<u64> {
        self.signals
            .range(..=seq)
            .rev()
            .find(|(_, signal)| signal.kind == "step.started" && signal.step.as_deref() == Some(step))
            .map(|(seq, _)| *seq)
    }

    fn rewind(&mut self, to: &str, at: Option<u64>) {
        let Some(cut) = at.or_else(|| self.cut(to)) else { return };
        self.signals.retain(|seq, _| *seq < cut);
        self.requested.retain(|seq, _| *seq < cut);
        self.requested_at.retain(|seq, _| *seq < cut);
        self.pauses.retain(|pause| pause.seq < cut);
        self.resolved.retain(|seq, _| *seq < cut);
        self.progress.retain(|seq, _| *seq < cut);
        self.order.retain(|seq| *seq < cut);
        self.rewinds += 1;
    }

    pub fn journal(&self) -> Vec<Value> {
        self.order
            .iter()
            .filter_map(|seq| {
                let request = self.requested.get(seq)?;
                let resolution = self.resolved.get(seq)?;
                Some(journal_entry(*seq, kind_of(&request.effect), &resolution.outcome, resolution.at))
            })
            .collect()
    }

    pub fn open(&self) -> impl Iterator<Item = &EffectRequest> {
        self.requested.values().filter(|request| !self.resolved.contains_key(&request.key.seq))
    }

    pub fn value(&self, seq: u64) -> Option<&Value> {
        match &self.resolved.get(&seq)?.outcome {
            EffectOutcome::Ok { value } => Some(value),
            _ => None,
        }
    }

    pub fn last_step(&self) -> Option<(String, String)> {
        self.signals
            .values()
            .rev()
            .find(|signal| signal.kind == "step.started")
            .and_then(|signal| Some((signal.step.clone()?, signal.branch.clone())))
    }

    pub fn step_type(&self, step: &str) -> Option<String> {
        self.signals
            .values()
            .rev()
            .find(|signal| signal.kind == "step.started" && signal.step.as_deref() == Some(step))
            .and_then(|signal| signal.data.get("type").and_then(Value::as_str).map(str::to_owned))
    }
}

#[cfg(test)]
mod tests {
    use domain::effect::{Effect, EffectRequest};
    use serde_json::json;

    use super::*;

    #[test]
    fn an_ask_pauses_its_run_from_when_it_opens_until_it_is_answered_or_the_run_closes_them() {
        let key = IdempotencyKey::new("i1", "plan", None, 1, 1);
        let agent = Effect::AgentRun { agent: "planner".into(), input: Value::Null };
        let progress = |data: Value| WorkflowEvent::EffectProgress { key: key.clone(), data };
        let mut ledger = Ledger::default();
        ledger.apply(&WorkflowEvent::EffectRequested(EffectRequest::new(key.clone(), agent)), 0);
        ledger.apply(&progress(json!({ "asks": { "a": { "since": 1 } } })), 10);
        ledger.apply(&progress(json!({ "asks": { "a": { "since": 1 } } })), 15);
        ledger.apply(&progress(json!({ "asks": { "a": null } })), 20);
        ledger.apply(&progress(json!({ "asks": { "b": { "since": 2 } } })), 30);
        ledger.apply(&progress(json!({ "asks": null })), 40);
        let spans: Vec<_> = ledger.pauses.iter().map(|pause| (pause.ask.as_str(), pause.from, pause.to)).collect();
        assert_eq!(spans, [("a", 10, Some(20)), ("b", 30, Some(40))]);
    }
}
