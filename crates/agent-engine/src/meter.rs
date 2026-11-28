use crate::RunGate;
use crate::model::{BoxFuture, CancellationToken, ModelApi, ModelError, OnDelta, Reply, TurnRequest};
use domain::billing::{Basis, Cost, ModelUsage};
use domain::origin::Attribution;
use domain::{EngineEvent, ModelProvider};
use llm_providers::model::Cache;
use llm_providers::priced;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct Ledger {
    costs: Mutex<Vec<Option<f64>>>,
}
impl Ledger {
    pub fn cost(&self) -> Option<f64> {
        self.costs.lock().ok()?.iter().copied().sum()
    }

    pub fn spent(&self) -> Result<Cost, ModelError> {
        let costs = self.costs.lock().map_err(|_| ModelError::Provider("usage ledger unavailable".into()))?;
        Ok(costs.iter().fold(Cost::default(), |spent, cost| spent.plus(Cost::priced(*cost, Basis::Unknown))))
    }
}
pub type Calls = Arc<dyn Fn(&TurnRequest, &Reply) + Send + Sync>;

pub struct Metered {
    pub api: Arc<dyn ModelApi>,
    pub ledger: Arc<Ledger>,
    pub provider: ModelProvider,
    pub gate: RunGate,
    pub model: domain::ModelOption,
    pub calls: Option<Calls>,
    pub budgets: Vec<domain::budget::Budget>,
    pub account: String,
    pub attribution: Attribution,
}
impl ModelApi for Metered {
    fn turn<'a>(
        &'a self,
        request: &'a TurnRequest,
        on: OnDelta<'a>,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Reply, ModelError>> {
        Box::pin(async move {
            if !self.budgets.is_empty() {
                let unpriced = domain::budget::Unpriced::configured();
                domain::budget::first_refusal(&self.budgets, self.ledger.spent()?, unpriced)
                    .map_err(ModelError::Quota)?;
            }
            let reply = self.api.turn(request, on, cancel).await?;
            self.record(request, &reply)?;
            Ok(reply)
        })
    }
}

impl Metered {
    fn record(&self, request: &TurnRequest, reply: &Reply) -> Result<(), ModelError> {
        if let Some(calls) = &self.calls {
            calls(request, reply);
        }
        let u = reply.usage;
        let provider = crate::providers::find(self.provider);
        let cost = estimate_cached(
            self.provider.id(),
            &ModelUsage {
                model: request.model.clone(),
                input: u.input,
                output: u.output,
                cache_read: u.cache_read,
                cache_write: u.cache_write,
                searches: u.searches,
                ..ModelUsage::default()
            },
            request.intent.cache,
        );
        let discount =
            provider.filter(|_| reply.batched).and_then(|p| llm_providers::later_discount(p, &request.model));
        let cost = crate::model::intent::discounted(cost, request.intent, discount);
        self.ledger.costs.lock().map_err(|_| ModelError::Provider("usage ledger unavailable".into()))?.push(cost);
        let served = self.served_by(request, reply);
        self.gate.emit(spent_by(
            crate::agent::current::agent(),
            ModelUsage {
                model: request.model.clone(),
                input: u.input,
                output: u.output,
                cache_read: u.cache_read,
                cache_write: u.cache_write,
                searches: u.searches,
                estimated_cost_usd: cost,
                basis: provider.map_or(Basis::Unknown, |provider| provider.basis()).label().into(),
                price_date: provider.map_or("", |provider| provider.price_date()).into(),
                provider: served.0,
                account: served.1,
                attribution: self.attribution(request),
            },
        ));
        Ok(())
    }

    fn attribution(&self, request: &TurnRequest) -> Attribution {
        request.intent.origin.map_or_else(|| self.attribution.clone(), |origin| self.attribution.as_origin(origin))
    }

    fn served_by(&self, request: &TurnRequest, reply: &Reply) -> (String, String) {
        let known: Vec<&str> = crate::providers::all().iter().map(|provider| provider.id()).collect();
        let main = self.provider.id();
        match domain::routing::route(&request.model, &known).provider {
            Some(other) if other != main => (other.to_owned(), reply.account.clone().unwrap_or_default()),
            _ => (main.to_owned(), reply.account.clone().unwrap_or_else(|| self.account.clone())),
        }
    }
}
pub fn estimate(provider: &str, usage: &ModelUsage) -> Option<f64> {
    estimate_cached(provider, usage, Cache::default())
}

pub fn estimate_cached(provider: &str, usage: &ModelUsage, cache: Cache) -> Option<f64> {
    let provider = crate::providers::all().into_iter().find(|known| known.id() == provider)?;
    let searched = provider.search_price().map_or(0.0, |price| domain::billing::searches_cost(price, usage.searches));
    let prompt = usage.input + usage.cache_read + usage.cache_write;
    priced(provider, &usage.model, prompt, cache)
        .map(|rate| rate.estimate(usage.input, usage.output, usage.cache_read, usage.cache_write) + searched)
}

fn spent_by(agent: Option<usize>, usage: ModelUsage) -> EngineEvent {
    let spent = EngineEvent::Usage(usage);
    match agent {
        Some(index) => EngineEvent::AgentEvent { index, event: Box::new(spent) },
        None => spent,
    }
}

impl std::fmt::Debug for Metered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Metered").field("provider", &self.provider).finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use domain::budget::Budget;
    use domain::quota::{QuotaError, Scope};

    use super::*;
    use crate::boundary::EventSink;
    use crate::model::BlockingTurn;
    use crate::model::scripted::ScriptedModel;
    use crate::transcript::Block;

    struct Quiet;

    impl EventSink for Quiet {
        fn emit(&self, _run: domain::RunId, _event: EngineEvent) {}
    }

    #[test]
    fn a_sub_agent_s_spend_reaches_the_caller_wrapped_as_its_agent_s_event_and_still_counts() {
        let usage = ModelUsage { estimated_cost_usd: Some(0.25), ..ModelUsage::default() };
        assert_eq!(spent_by(None, usage.clone()), EngineEvent::Usage(usage.clone()));
        let wrapped = spent_by(Some(2), usage.clone());
        assert_eq!(wrapped, EngineEvent::AgentEvent { index: 2, event: Box::new(EngineEvent::Usage(usage)) });
        assert_eq!(domain::budget::spent(&wrapped).known_micros, 250_000);
    }

    fn metered(budget: Option<Budget>) -> Metered {
        let budgets = budget.into_iter().collect();
        Metered {
            api: Arc::new(ScriptedModel::new([ScriptedModel::reply(vec![Block::Text("ok".into())])])),
            ledger: Arc::default(),
            provider: ModelProvider::named("scripted"),
            gate: RunGate::new(domain::RunId::new(), Arc::new(Quiet)),
            model: domain::ModelOption::new("m", "M"),
            calls: None,
            budgets,
            account: "a@x.io".into(),
            attribution: Attribution::default(),
        }
    }

    #[test]
    fn a_call_is_charged_to_the_account_that_answered_or_else_the_first_one() {
        let meter = metered(None);
        let stop = CancellationToken::new();
        let request = TurnRequest {
            model: "m".into(),
            system: String::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            effort: domain::Effort::Medium,
            max_tokens: 100,
            intent: Default::default(),
        };
        let reply = meter.api.turn_blocking(&request, &mut |_| {}, &stop).expect("reply");
        assert_eq!(meter.served_by(&request, &reply), ("scripted".to_owned(), "a@x.io".to_owned()));
        let pooled = Reply { account: Some("b@x.io".into()), ..reply };
        assert_eq!(meter.served_by(&request, &pooled), ("scripted".to_owned(), "b@x.io".to_owned()));
    }

    fn spent(usd: f64) -> Cost {
        Cost::priced(Some(usd), Basis::Api)
    }

    #[test]
    fn a_spent_budget_stops_the_next_call_before_it_is_made() {
        let request = TurnRequest {
            model: "m".into(),
            system: String::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            effort: domain::Effort::Medium,
            max_tokens: 100,
            intent: Default::default(),
        };
        let stop = CancellationToken::new();

        let used_up = metered(Some(Budget::session(1.0, spent(1.0))));
        let refused = used_up.turn_blocking(&request, &mut |_| {}, &stop).expect_err("refused");
        assert!(matches!(refused, ModelError::Quota(QuotaError::Budget { .. })), "{refused}");
        assert!(refused.to_string().contains("budget"), "{refused}");

        let within = metered(Some(Budget::session(1.0, spent(0.5))));
        assert!(within.turn_blocking(&request, &mut |_| {}, &stop).is_ok());
        assert!(metered(None).turn_blocking(&request, &mut |_| {}, &stop).is_ok());
    }

    #[test]
    fn with_unpriced_calls_blocked_a_call_without_a_price_stops_the_next_one() {
        assert_eq!(domain::budget::Unpriced::configured(), domain::budget::Unpriced::Block);
        let request = TurnRequest {
            model: "m".into(),
            system: String::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            effort: domain::Effort::Medium,
            max_tokens: 100,
            intent: Default::default(),
        };
        let stop = CancellationToken::new();
        let meter = metered(Some(Budget::session(10.0, spent(0.5))));
        assert!(
            meter.turn_blocking(&request, &mut |_| {}, &stop).is_ok(),
            "the first call has nothing unpriced before it"
        );
        assert_eq!(meter.ledger.spent().expect("ledger").unpriced_calls, 1, "the scripted model has no price");
        let refused = meter.turn_blocking(&request, &mut |_| {}, &stop).expect_err("blocked");
        let ModelError::Quota(QuotaError::Budget { scope, spent, .. }) = &refused else {
            panic!("not a budget refusal: {refused:?}");
        };
        assert_eq!((*scope, spent.known_micros, spent.unpriced_calls), (Scope::Session, 500_000, 1));
        assert!(matches!(domain::FailureKind::from(&refused), domain::FailureKind::Budget { scope: Scope::Session, .. }));
    }

    #[test]
    fn every_provider_with_a_price_table_declares_its_date_and_basis() {
        let priced: Vec<_> =
            crate::providers::all().into_iter().filter(|provider| !provider.prices().is_empty()).collect();
        assert!(!priced.is_empty());
        for provider in priced {
            assert!(!provider.price_date().is_empty(), "{} declares no price date", provider.id());
            assert_ne!(provider.basis(), Basis::Unknown, "{}", provider.id());
        }
    }

    #[test]
    fn the_low_priority_discount_counts_only_when_the_call_went_through_a_batch() {
        let request = TurnRequest {
            model: "claude-sonnet-5-5".into(),
            system: String::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            effort: domain::Effort::Medium,
            max_tokens: 100,
            intent: crate::model::Intent::LATER,
        };
        let cost_of = |batched: bool| {
            let meter = Metered { provider: ModelProvider::named("claude-subscription"), ..metered(None) };
            let reply = ScriptedModel::reply(vec![Block::Text("ok".into())]).expect("reply");
            meter.record(&request, &Reply { batched, ..reply }).expect("recorded");
            meter.ledger.cost().expect("priced")
        };
        let (now, batched) = (cost_of(false), cost_of(true));
        assert!(now > 0.0);
        assert!((batched - now * 0.5).abs() < 1e-12, "{batched} vs {now}");
    }
}
