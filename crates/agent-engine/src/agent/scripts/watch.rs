use std::sync::Arc;

use domain::watch::{Plan, Step, Watching};
use serde_json::{Value, json};

use super::{Script, Within};
use crate::agent::Agent;
use crate::agent::run::ThisRun;
use crate::tools::Tool;

pub(super) const SPACE: &str = "watch";

pub(super) fn call<'env>(
    agent: &'env Agent<'_>,
    within: Within<'_, 'env>,
    _script: &Script<'_>,
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    match name {
        "start" => Ok(agent.start_watch(within.id, arguments, within.run)),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn status(status: &str, reason: impl Into<String>) -> Value {
    json!({ "status": status, "reason": reason.into() })
}

impl Agent<'_> {
    fn start_watch(&self, id: &str, input: &Value, run: &ThisRun) -> Value {
        let mut plan = match Plan::parse(input) {
            Ok(plan) => plan,
            Err(error) => return status("refused", error),
        };
        let active = crate::tools::watch::active(self.workspace.credentials.client);
        if let Some(refusal) = domain::watch::duplicate(&plan.tool, &plan.input, &active) {
            return status("refused", refusal);
        }
        let name = plan.tool.clone();
        let named = |status: &str| json!({ "status": status, "tool": name });
        let Some(target) = self.tool(run, &name) else { return named("missing") };
        if !self.offers(&name) {
            return named("unloaded");
        }
        let reads = crate::tools::changes_nothing(target, &plan.input, self.workspace);
        let Some(kept) = keepable(&name, target, reads) else { return named("unwatchable") };
        let model = Some((self.api, &self.request));
        let hook = match crate::tools::watch::before(&name, &plan.input, self.workspace, model) {
            Ok(hook) => hook,
            Err(refusal) => return status("blocked", refusal),
        };
        if let Some(updated) = hook.updated_input {
            plan.input = updated;
        }
        let summary = target.summary(&plan.input);
        let shown = format!("{}: {summary}", plan.explained());
        let permitting = crate::agent::permission::Permitting {
            id,
            subject: &summary,
            shown: Some(&shown),
            parent: &[],
            hook_permission: hook.permission,
        };
        let first = self.permitted(&permitting, target, &plan.input);
        let first = crate::tools::watch::after(&name, &plan.input, first, self.workspace, model);
        if first.is_error {
            return status("failed", first.content);
        }
        let mut watching = Watching::new(plan);
        if let step @ Step::Met(_) = watching.observe(Ok(&first.content)) {
            return json!({ "status": "met", "told": watching.told(&step).unwrap_or_default() });
        }
        let explained = watching.plan.explained();
        let watch = crate::tools::watch::start(watching, kept, self.workspace);
        run.watches.started(watch.clone());
        json!({ "status": "watching", "id": watch, "explained": explained })
    }
}

fn keepable(name: &str, tool: &dyn Tool, reads: bool) -> Option<Arc<dyn Tool>> {
    if tool.script().is_some_and(|script| super::spaces::reached(&script.source)) {
        return None;
    }
    tool.kept().or_else(|| {
        let standard = reads.then(|| crate::tools::named(name)).flatten()?;
        Some(Arc::from(standard))
    })
}
