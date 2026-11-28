use domain::data_policy::Requires;
use domain::overrun::Overrun;
use domain::permission::Surface;
use domain::quota::Scope;
use domain::workflow::Assignee;
use domain::{AskId, AskKind, Decision, ModelOption, Prompt, PromptRequest, SessionId};
use serde_json::{Value, json};

use crate::agent::{AgentEffect, Collected, Failure, RESUME};
use crate::asking::Asking;
use crate::ceiling::{Reached, budget_reached};

pub const ASK_TOOL: &str = "budget";

pub fn granted_of(progress: &Value) -> Vec<(Scope, f64)> {
    let listed = progress["overrun"]["granted"].as_array().map(Vec::as_slice).unwrap_or_default();
    listed.iter().filter_map(|grant| Some((Scope::named(grant["scope"].as_str()?)?, grant["usd"].as_f64()?))).collect()
}

struct Told<'a> {
    declared: &'a Overrun,
    reached: &'a Reached,
    went: bool,
    skipped: Option<String>,
    fell: bool,
}

impl AgentEffect<'_> {
    pub(crate) async fn bounded(&self, request: &PromptRequest) -> Result<Collected, Failure> {
        let declared = Overrun::declared(&self.command.args["onBudget"]);
        let mut next = request.clone();
        let mut fell = self.fallen(&declared, &mut next);
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
            if went {
                fell |= matches!(declared, Overrun::Smaller(_));
                self.extend(&next, reached.scope);
            }
            self.told(&Told { declared: &declared, reached: &reached, went, skipped, fell });
            if !went {
                return Err(Failure::Blocked(reason));
            }
            next.prompt = Prompt::typed(RESUME.to_owned());
            next.session.resume = self.session().map(SessionId).or(next.session.resume);
        }
    }

    fn progress(&self) -> Value {
        let Ok(state) = self.shared.item(self.item) else { return Value::Null };
        state.effects.get(&self.key.to_string()).map(|entry| entry.progress.clone()).unwrap_or_default()
    }

    fn session(&self) -> Option<String> {
        self.progress()["session"].as_str().map(str::to_owned)
    }

    fn fallen(&self, declared: &Overrun, next: &mut PromptRequest) -> bool {
        let Overrun::Smaller(model) = declared else { return false };
        let fallen = self.progress()["overrun"]["fell"].as_str() == Some(model.as_str());
        if fallen && let Some(why) = self.fall_to(next, model) {
            tracing::warn!(target: "audit", agent = self.command.text("name"), item = %self.item, %why, "budget.overrun.restored");
        }
        fallen
    }

    fn extend(&self, request: &PromptRequest, scope: Scope) {
        let Some(charge) = self.charge.get() else { return };
        let budgets = self.budgeted(request).policy.budgets;
        if let Some(found) = budgets.iter().find(|budget| budget.scope == scope) {
            charge.extend(scope, found.limit_usd);
        }
    }

    async fn approved(&self, reason: &str) -> bool {
        let Ok(state) = self.shared.item(self.item) else { return false };
        if crate::policy::surface_of(&state) == Surface::Routine {
            return false;
        }
        let asked = self.charge.get().map_or(0, crate::spend::Charge::extensions);
        let id = AskId(format!("{ASK_TOOL}-{}-{asked}", self.key));
        let asking = Asking { shared: self.shared, item: self.item, key: self.key, id };
        let decided = asking.decided_by(AskKind::permission(ASK_TOOL, reason, false), Some(Assignee::Admins)).await;
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

    fn told(&self, told: &Told<'_>) {
        let Told { declared, reached, went, ref skipped, fell } = *told;
        tracing::info!(
            target: "audit",
            agent = self.command.text("name"),
            item = %self.item,
            step = self.key.step,
            outcome = if went { "allowed" } else { "denied" },
            scope = reached.scope.word(),
            action = declared.word(),
            policy = declared.word(),
            went,
            skipped = skipped.as_deref().unwrap_or_default(),
            "budget.overrun"
        );
        if *declared == Overrun::Stop {
            return;
        }
        let granted: Vec<Value> = self
            .charge
            .get()
            .map(crate::spend::Charge::granted)
            .unwrap_or_default()
            .into_iter()
            .map(|(scope, usd)| json!({ "scope": scope.word(), "usd": usd }))
            .collect();
        let mut data =
            json!({ "action": declared.word(), "scope": reached.scope.word(), "went": went, "granted": granted });
        if let Overrun::Smaller(model) = declared {
            data["model"] = json!(model);
            data["fell"] = if fell { json!(model) } else { Value::Null };
        }
        if let Some(why) = skipped {
            data["skipped"] = json!(why);
        }
        self.note(json!({ "overrun": data }));
    }
}
