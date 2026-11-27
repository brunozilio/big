use domain::small_model::{Offered, SmallChoice, choose};
use domain::{ModelOption, ModelProvider};

use crate::model::{BoxFuture, CancellationToken, ModelApi, ModelError, OnDelta, Reply, TurnRequest};

pub struct OrSession<'a> {
    pub api: &'a dyn ModelApi,
    pub session: String,
}

impl ModelApi for OrSession<'_> {
    fn turn<'b>(
        &'b self,
        request: &'b TurnRequest,
        on: OnDelta<'b>,
        cancel: &'b CancellationToken,
    ) -> BoxFuture<'b, Result<Reply, ModelError>> {
        Box::pin(async move {
            match self.api.turn(request, &mut *on, cancel).await {
                Err(error) if error != ModelError::Cancelled && request.model != self.session => {
                    tracing::warn!(%error, model = %request.model, "the small model failed; the session's model answers");
                    let again = TurnRequest { model: self.session.clone(), ..request.clone() };
                    self.api.turn(&again, on, cancel).await
                }
                answered => answered,
            }
        })
    }
}

fn bare(model: &str) -> &str {
    model.rsplit(':').next().unwrap_or(model)
}

pub fn pick(provider: ModelProvider, session: &str, configured: Option<&str>, catalog: &[ModelOption]) -> SmallChoice {
    if let Some(elsewhere) = configured.filter(|name| super::roles::elsewhere(name, provider)) {
        return SmallChoice { model: Some(elsewhere.to_owned()), ..SmallChoice::default() };
    }
    let found = crate::providers::find(provider);
    let vendor = found.and_then(|provider| llm_providers::vendor(provider.vendor()));
    let configured = configured.map(|name| vendor.and_then(|vendor| vendor.alias(name)).unwrap_or(name));
    let declared = found.and_then(|provider| provider.small_model());
    let offered: Vec<Offered> = catalog
        .iter()
        .filter(|model| model.provider == provider)
        .map(|model| Offered { id: &model.id, input_rate: input_rate(found.map(|p| p.vendor()), &model.id) })
        .collect();
    choose(session, configured, declared, &offered)
}

fn input_rate(provider_vendor: Option<&str>, id: &str) -> Option<u64> {
    let model = bare(id);
    let dashed = model.replace('.', "-");
    [model, dashed.as_str()]
        .into_iter()
        .find_map(|name| {
            provider_vendor
                .and_then(|vendor| llm_providers::rates(vendor, name, 0))
                .or_else(|| llm_providers::owner(name).and_then(|owner| llm_providers::rates(owner.id(), name, 0)))
        })
        .map(|rates| rates.input)
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

    #[test]
    fn a_failed_small_model_hands_the_call_to_the_session_s_model() {
        let failed = Err(ModelError::Provider("unknown model".into()));
        let scripted = ScriptedModel::new([failed, ScriptedModel::reply(vec![Block::Text("ok".into())])]);
        let api = OrSession { api: &scripted, session: "big".into() };
        let reply = api.turn_blocking(&asked("small"), &mut |_| {}, &CancellationToken::new()).expect("answered");
        assert_eq!(reply.message.text(), "ok");
        let models: Vec<String> = scripted.asked().into_iter().map(|request| request.model).collect();
        assert_eq!(models, ["small", "big"]);
    }

    #[test]
    fn the_session_s_own_model_and_a_cancel_are_not_asked_again() {
        for (model, error) in [("big", ModelError::Provider("down".into())), ("small", ModelError::Cancelled)] {
            let scripted = ScriptedModel::new([Err(error)]);
            let api = OrSession { api: &scripted, session: "big".into() };
            assert!(api.turn_blocking(&asked(model), &mut |_| {}, &CancellationToken::new()).is_err());
            assert_eq!(scripted.asked().len(), 1);
        }
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
