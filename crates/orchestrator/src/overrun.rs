use domain::data_policy::Requires;
use domain::overrun::Overrun;
use domain::permission::Surface;
use domain::{AskId, AskKind, Decision, ModelOption, Prompt, PromptRequest, SessionId};
use serde_json::json;

use crate::agent::{AgentEffect, Collected, Failure, RESUME};
use crate::asking::Asking;
use crate::ceiling::{Reached, budget_reached};

pub const ASK_TOOL: &str = "budget";

impl AgentEffect<'_> {
    pub(crate) async fn bounded(&self, request: &PromptRequest) -> Result<Collected, Failure> {
        let declared = Overrun::declared(&self.command.args["onBudget"]);
        let mut next = request.clone();
        let mut fell = false;
        loop {
            let reason = match self.turn(&next).await {
                Err(Failure::Blocked(reason)) => reason,
                other => return other,
            };
            let Some(reached) = budget_reached(&reason) else { return Err(Failure::Blocked(reason)) };
            let (went, skipped) = match &declared {
                Overrun::Stop => (false, None),
                Overrun::Ask => (self.approved(&reason).await, None),
                Overrun::Smaller(_) if fell => (false, None),
                Overrun::Smaller(model) => {
                    self.fall_to(&mut next, model).map_or((true, None), |why| (false, Some(why)))
                }
            };
            self.told(&declared, &reached, went, skipped);
            if !went {
                return Err(Failure::Blocked(reason));
            }
            fell |= matches!(declared, Overrun::Smaller(_));
            if let Some(charge) = self.charge.get() {
                charge.extend(reached.scope, reached.limit_usd);
            }
            next.prompt = Prompt::typed(RESUME.to_owned());
            next.session.resume = self.session().map(SessionId).or(next.session.resume);
        }
    }

    fn session(&self) -> Option<String> {
        let state = self.shared.item(self.item).ok()?;
        let entry = state.effects.get(&self.key.to_string())?;
        entry.progress["session"].as_str().map(str::to_owned)
    }

    async fn approved(&self, reason: &str) -> bool {
        let Ok(state) = self.shared.item(self.item) else { return false };
        if crate::policy::surface_of(&state) == Surface::Routine {
            return false;
        }
        let asked = self.charge.get().map_or(0, crate::spend::Charge::extensions);
        let id = AskId(format!("{ASK_TOOL}-{}-{asked}", self.key));
        let asking = Asking { shared: self.shared, item: self.item, key: self.key, id };
        let decided = asking.decided(AskKind::permission(ASK_TOOL, reason, false)).await;
        matches!(decided, Some(Decision::Allow | Decision::AllowAlways | Decision::Approve(_)))
    }

    fn fall_to(&self, next: &mut PromptRequest, model: &str) -> Option<String> {
        let requires = Requires::configured().stricter(&next.session.requires);
        if let Some(why) = agent_engine::data_gate::unmet(&requires, model, next.session.model.provider) {
            return Some(format!("{model}: {why}"));
        }
        match agent_engine::tiers::current().find(model) {
            Some(tier) => next.session.tier = Some(tier.name.clone()),
            None => {
                next.session.tier = None;
                let listed = self.shared.config.engine.models().into_iter().find(|option| option.id == model);
                next.session.model = listed.unwrap_or_else(|| ModelOption::unlisted(model));
            }
        }
        None
    }

    fn told(&self, declared: &Overrun, reached: &Reached, went: bool, skipped: Option<String>) {
        tracing::info!(
            target: "audit",
            item = %self.item,
            step = self.key.step,
            scope = reached.scope.word(),
            action = declared.word(),
            went,
            skipped = skipped.as_deref().unwrap_or_default(),
            "budget.overrun"
        );
        if *declared == Overrun::Stop {
            return;
        }
        let mut data = json!({ "action": declared.word(), "scope": reached.scope.word(), "went": went });
        if let Overrun::Smaller(model) = declared {
            data["model"] = json!(model);
        }
        if let Some(why) = skipped {
            data["skipped"] = json!(why);
        }
        self.note(json!({ "overrun": data }));
    }
}
