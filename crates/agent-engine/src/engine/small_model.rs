use domain::data_policy::{Requires, Rerouted, Skipped};
use domain::small_model::{Offered, SmallChoice, choose};
use domain::{EngineEvent, ModelOption, ModelProvider};

use crate::model::{BoxFuture, CancellationToken, ModelApi, ModelError, OnDelta, Reply, TurnRequest};

pub struct OrSession<'a> {
    pub api: &'a dyn ModelApi,
    pub session: String,
    pub provider: ModelProvider,
    pub requires: Requires,
    pub emit: &'a (dyn Fn(EngineEvent) + Sync),
}

impl OrSession<'_> {
    fn unmet(&self, model: &str) -> Option<Skipped> {
        let why = crate::data_gate::unmet(&self.requires, model, self.provider)?;
        Some(Skipped { model: model.to_owned(), why })
    }

    fn handed_over(&self, from: &str, failure: &str, mut skipped: Vec<Skipped>) -> Result<(), ModelError> {
        let blocked = self.unmet(&self.session);
        let to = blocked.is_none().then(|| self.session.clone());
        skipped.extend(blocked);
        let decision = Rerouted { from: from.to_owned(), to: to.clone(), failure: failure.to_owned(), skipped };
        (self.emit)(EngineEvent::Rerouted(decision.clone()));
        match to {
            Some(model) => {
                (self.emit)(EngineEvent::FellBack(model));
                Ok(())
            }
            None => Err(crate::data_gate::refused(&decision.skipped)),
        }
    }
}

impl ModelApi for OrSession<'_> {
    fn turn<'b>(
        &'b self,
        request: &'b TurnRequest,
        on: OnDelta<'b>,
        cancel: &'b CancellationToken,
    ) -> BoxFuture<'b, Result<Reply, ModelError>> {
        Box::pin(async move {
            if request.model == self.session {
                if let Some(skipped) = self.unmet(&self.session) {
                    return Err(crate::data_gate::refused(&[skipped]));
                }
                return self.api.turn(request, on, cancel).await;
            }
            let (failure, skipped) = match self.unmet(&request.model) {
                Some(skipped) => (domain::data_policy::UNMET.to_owned(), vec![skipped]),
                None => match self.api.turn(request, &mut *on, cancel).await {
                    Err(error) if error != ModelError::Cancelled => {
                        tracing::warn!(%error, model = %request.model, "the small model failed; the session's model answers");
                        (error.failure().map_or("provider", |failure| failure.label()).to_owned(), Vec::new())
                    }
                    answered => return answered,
                },
            };
            self.handed_over(&request.model, &failure, skipped)?;
            let again = TurnRequest { model: self.session.clone(), ..request.clone() };
            self.api.turn(&again, on, cancel).await
        })
    }
}

pub fn pick(provider: ModelProvider, session: &str, configured: Option<&str>, catalog: &[ModelOption]) -> SmallChoice {
    if let Some(elsewhere) = configured.filter(|name| super::roles::elsewhere(name, provider)) {
        return SmallChoice { model: Some(elsewhere.to_owned()), ..SmallChoice::default() };
    }
    let found = crate::providers::find(provider);
    let configured = configured.map(|name| found.and_then(|provider| provider.alias(name)).unwrap_or(name));
    let declared = found.and_then(|provider| provider.small_model());
    let offered: Vec<Offered> = catalog
        .iter()
        .filter(|model| model.provider == provider)
        .map(|model| Offered { id: &model.id, input_rate: input_rate(found, &model.id) })
        .collect();
    choose(session, configured, declared, &offered)
}

pub fn within(small: Option<String>, requires: &Requires, provider: ModelProvider) -> Option<String> {
    small.filter(|model| match crate::data_gate::unmet(requires, model, provider) {
        None => true,
        Some(why) => {
            tracing::warn!(model, %why, "the small model does not meet the data policy; the session's model answers");
            false
        }
    })
}

fn input_rate(provider: Option<&dyn llm_providers::Provider>, id: &str) -> Option<u64> {
    let priced = llm_providers::priced(provider?, id, 0, llm_providers::model::Cache::default());
    priced.map(|rates| rates.input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BlockingTurn;
    use crate::model::scripted::ScriptedModel;
    use crate::transcript::{Block, Message};

    fn asked(model: &str) -> TurnRequest {
        TurnRequest {
            model: model.into(),
            system: String::new(),
            messages: vec![Message::user("hi")],
            tools: Vec::new(),
            effort: domain::Effort::Low,
            max_tokens: 10,
            intent: crate::model::Intent::ONCE,
        }
    }

    fn session_of<'a>(api: &'a ScriptedModel, emit: &'a (dyn Fn(EngineEvent) + Sync)) -> OrSession<'a> {
        let provider = ModelProvider::new("keyed");
        OrSession { api, session: "big".into(), provider, requires: Requires::default(), emit }
    }

    #[test]
    fn a_failed_small_model_hands_the_call_to_the_session_s_model() {
        let failed = Err(ModelError::Provider("unknown model".into()));
        let scripted = ScriptedModel::new([failed, ScriptedModel::reply(vec![Block::Text("ok".into())])]);
        let api = session_of(&scripted, &|_| {});
        let reply = api.turn_blocking(&asked("small"), &mut |_| {}, &CancellationToken::new()).expect("answered");
        assert_eq!(reply.message.text(), "ok");
        let models: Vec<String> = scripted.asked().into_iter().map(|request| request.model).collect();
        assert_eq!(models, ["small", "big"]);
    }

    #[test]
    fn the_session_s_own_model_and_a_cancel_are_not_asked_again() {
        for (model, error) in [("big", ModelError::Provider("down".into())), ("small", ModelError::Cancelled)] {
            let scripted = ScriptedModel::new([Err(error)]);
            let api = session_of(&scripted, &|_| {});
            assert!(api.turn_blocking(&asked(model), &mut |_| {}, &CancellationToken::new()).is_err());
            assert_eq!(scripted.asked().len(), 1);
        }
    }

    fn eu() -> Requires {
        Requires { regions: Some(vec!["eu".into()]), ..Requires::default() }
    }

    #[test]
    fn the_hand_over_to_the_session_s_model_is_told() {
        let heard = std::sync::Mutex::new(Vec::new());
        let emit = |event: EngineEvent| heard.lock().expect("heard").push(event);
        let scripted = ScriptedModel::new([
            Err(ModelError::Overloaded("busy".into())),
            ScriptedModel::reply(vec![Block::Text("ok".into())]),
        ]);
        let api = session_of(&scripted, &emit);
        api.turn_blocking(&asked("small"), &mut |_| {}, &CancellationToken::new()).expect("answered");
        let heard = heard.lock().expect("heard").clone();
        let decision =
            Rerouted { from: "small".into(), to: Some("big".into()), failure: "overloaded".into(), skipped: vec![] };
        assert_eq!(heard, [EngineEvent::Rerouted(decision), EngineEvent::FellBack("big".into())]);
    }

    #[test]
    fn a_small_model_outside_the_policy_is_never_asked() {
        crate::providers::install_testing();
        let scripted = ScriptedModel::new([ScriptedModel::reply(vec![Block::Text("ok".into())])]);
        let api =
            OrSession { provider: ModelProvider::new("keyed-too"), requires: eu(), ..session_of(&scripted, &|_| {}) };
        let small = asked("keyed:small");
        let reply = api.turn_blocking(&small, &mut |_| {}, &CancellationToken::new()).expect("answered");
        assert_eq!(reply.message.text(), "ok");
        let models: Vec<String> = scripted.asked().into_iter().map(|request| request.model).collect();
        assert_eq!(models, ["big"]);
    }

    #[test]
    fn without_a_model_that_meets_the_policy_the_call_fails_typed() {
        crate::providers::install_testing();
        let scripted = ScriptedModel::new([]);
        let api = OrSession { requires: eu(), ..session_of(&scripted, &|_| {}) };
        let refused = api.turn_blocking(&asked("small"), &mut |_| {}, &CancellationToken::new());
        assert!(matches!(refused, Err(ModelError::DataPolicyUnmet(_))), "{refused:?}");
        assert!(scripted.asked().is_empty());
    }

    #[test]
    fn a_small_model_outside_the_policy_is_dropped_so_the_session_s_model_answers() {
        crate::providers::install_testing();
        let session = ModelProvider::new("keyed-too");
        assert_eq!(within(Some("keyed:small".into()), &eu(), session), None);
        assert_eq!(within(Some("small".into()), &eu(), session).as_deref(), Some("small"));
        assert_eq!(within(Some("keyed:small".into()), &Requires::default(), session).as_deref(), Some("keyed:small"));
        assert_eq!(within(None, &eu(), session), None);
    }

    fn offered(provider: ModelProvider, id: &str) -> ModelOption {
        ModelOption::new(id, id).served_by(provider)
    }

    #[test]
    fn each_provider_brings_its_own_small_model() {
        let claude = ModelProvider::new("claude-subscription");
        assert_eq!(pick(claude, "claude-opus-5-5", None, &[]).model.as_deref(), Some("claude-haiku-5-5"));
        let chatgpt = ModelProvider::new("openai-subscription");
        assert_eq!(pick(chatgpt, "gpt-6.1-sol", None, &[]).model.as_deref(), Some("gpt-6-luna"));
    }

    #[test]
    fn a_subscription_without_the_declared_model_gets_its_cheapest_priced_one() {
        let chatgpt = ModelProvider::new("openai-subscription");
        let catalog = [offered(chatgpt, "gpt-6.1-sol"), offered(chatgpt, "gpt-6.1-terra")];
        assert_eq!(pick(chatgpt, "gpt-6.1-sol", None, &catalog).model.as_deref(), Some("gpt-6.1-terra"));
    }

    #[test]
    fn a_small_model_on_another_connected_provider_is_kept_as_named() {
        let chatgpt = ModelProvider::new("openai-subscription");
        let catalog = [offered(chatgpt, "gpt-6.1-sol"), offered(chatgpt, "gpt-6.1-terra")];
        let grok = "grok-subscription:grok-4.6-fast";
        let choice = pick(chatgpt, "gpt-6.1-sol", Some(grok), &catalog);
        assert_eq!((choice.model.as_deref(), choice.ignored), (Some(grok), None));
    }

    #[test]
    fn kiro_declares_haiku_under_its_own_name() {
        let kiro = ModelProvider::new("kiro-subscription");
        assert_eq!(pick(kiro, "kiro-subscription:auto", None, &[]).model.as_deref(), Some("claude-haiku-4.5"));
    }

    #[test]
    fn copilot_takes_the_cheapest_model_it_lists_whoever_makes_it() {
        let copilot = ModelProvider::new("copilot-subscription");
        let catalog = [
            offered(copilot, "copilot-subscription:claude-sonnet-4.5"),
            offered(copilot, "copilot-subscription:claude-haiku-4.5"),
            offered(copilot, "copilot-subscription:gpt-6.1-sol"),
            offered(copilot, "copilot-subscription:unpriced-model"),
        ];
        let chosen = pick(copilot, "copilot-subscription:claude-sonnet-4.5", None, &catalog);
        assert_eq!(chosen.model.as_deref(), Some("copilot-subscription:claude-haiku-4.5"));
    }

    #[test]
    fn an_alias_is_resolved_before_it_is_checked() {
        let claude = ModelProvider::new("claude-subscription");
        let catalog = [offered(claude, "claude-haiku-5-5"), offered(claude, "claude-opus-5-5")];
        let chosen = pick(claude, "claude-opus-5-5", Some("haiku"), &catalog);
        assert_eq!((chosen.model.as_deref(), chosen.ignored), (Some("claude-haiku-5-5"), None));
        let chatgpt = ModelProvider::new("openai-subscription");
        let elsewhere = pick(chatgpt, "gpt-6.1-sol", Some("haiku"), &[offered(chatgpt, "gpt-6-luna")]);
        assert!(elsewhere.ignored.is_some());
        assert_eq!(elsewhere.model.as_deref(), Some("gpt-6-luna"));
    }
}
