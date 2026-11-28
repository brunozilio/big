mod answers;
mod brief;
mod failing;
mod handlers;
mod model;
mod rewrites;
mod script_hook;
mod scripts;
mod validate;

use self::answers::absorbed;
pub use self::brief::Briefed;
pub use self::failing::fails_closed;
pub use self::handlers::{HookRunError, command, http};
pub(crate) use self::script_hook::declared;
pub use self::script_hook::script;
pub use self::scripts::BUILTIN as BUILTIN_SCRIPT;
pub(crate) use self::scripts::{runs_from, source as script_source, target as script_target};
pub(crate) use self::validate::TIMEOUT_SECONDS;
use crate::tools::Workspace;
pub use domain::hooks::{HandlerKind, HookEvent};
use domain::permission::RuleKind as PermissionDecision;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hooks(pub BTreeMap<HookEvent, Vec<Group>>);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    #[serde(default)]
    pub matcher: String,
    pub hooks: Vec<Handler>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handler {
    #[serde(rename = "type")]
    pub kind: HandlerKind,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub allowed_env_vars: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(default = "fails_closed", rename = "failClosed")]
    pub fail_closed: bool,
    #[serde(default, skip_serializing, deserialize_with = "accepted_and_dropped")]
    pub status_message: (),
}
fn default_timeout() -> u64 {
    600
}
fn accepted_and_dropped<'de, D: serde::Deserializer<'de>>(written: D) -> Result<(), D::Error> {
    serde::de::IgnoredAny::deserialize(written).map(|_| ())
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct HookOutcome {
    pub blocked: Option<String>,
    pub context: String,
    pub updated_input: Option<Value>,
    pub permission: Option<PermissionDecision>,
    pub updated_output: Option<Value>,
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<domain::Effort>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub brief: Option<Value>,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub history: Option<Value>,
    #[serde(default)]
    pub prompt: Option<String>,
}
impl HookOutcome {
    fn without_trailing_whitespace(mut self) -> Self {
        self.blocked = self.blocked.map(|reason| reason.trim_end().to_owned());
        self.context.truncate(self.context.trim_end().len());
        self
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct CommandOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}
type Model<'a> = Option<(&'a dyn crate::model::ModelApi, &'a crate::model::TurnRequest)>;

#[derive(Debug, PartialEq, Eq)]
enum Handled {
    GoOn,
    Blocked,
}

impl Group {
    fn applies_to(&self, subject: &str) -> Result<bool, String> {
        if self.matcher.is_empty() || self.matcher == "*" {
            return Ok(true);
        }
        Ok(regex::Regex::new(&self.matcher).map_err(|e| e.to_string())?.is_match(subject))
    }
}

impl Handler {
    fn runs(&self, input: &Value, workspace: &Workspace, model: Model<'_>) -> Result<CommandOutput, String> {
        match self.kind {
            HandlerKind::Http => http(self, input, workspace).map_err(String::from),
            HandlerKind::Script => script(self, input, workspace, model),
            HandlerKind::Command => command(self, input, workspace).map_err(String::from),
            HandlerKind::Prompt | HandlerKind::Agent => self.judged(input, workspace, model),
        }
    }

    fn judged(&self, input: &Value, workspace: &Workspace, model: Model<'_>) -> Result<CommandOutput, String> {
        let (api, template) = model.ok_or("model hook requires an active model context")?;
        let verdict = model::evaluate(self, input, workspace, api, template)?;
        let stdout = if verdict["ok"] == true {
            "{}".into()
        } else {
            let reason = verdict["reason"].as_str().unwrap_or("Model hook rejected the action");
            json!({"decision":"block","reason":reason}).to_string()
        };
        Ok(CommandOutput { code: Some(0), stdout, stderr: String::new() })
    }
}

impl Hooks {
    pub fn listed(&self) -> Vec<domain::permission::HookInfo> {
        let mut listed = Vec::new();
        for (event, groups) in &self.0 {
            for group in groups {
                for handler in &group.hooks {
                    let runs = match handler.kind {
                        HandlerKind::Http => handler.url.clone().unwrap_or_default(),
                        HandlerKind::Prompt | HandlerKind::Agent => {
                            let prompt: String =
                                handler.prompt.as_deref().unwrap_or_default().chars().take(60).collect();
                            format!("{}: {prompt}", handler.kind)
                        }
                        HandlerKind::Command | HandlerKind::Script => handler.command.clone(),
                    };
                    listed.push(domain::permission::HookInfo { event: *event, matcher: group.matcher.clone(), runs });
                }
            }
        }
        listed
    }

    pub fn parse(value: &Value) -> Result<Self, String> {
        validate::named(value)?;
        let events: BTreeMap<HookEvent, Vec<Group>> =
            serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        for (event, groups) in &events {
            for group in groups {
                validate::matcher(*event, &group.matcher)?;
            }
            for (at, hook) in groups.iter().flat_map(|group| &group.hooks).enumerate() {
                validate::hook(*event, at, hook)?;
            }
        }
        Ok(Self(events))
    }
    pub fn extend(&mut self, other: &Self) {
        for (event, groups) in &other.0 {
            self.0.entry(*event).or_default().extend(groups.clone());
        }
    }
    pub fn listens_to(&self, event: HookEvent) -> bool {
        self.0.contains_key(&event)
    }
    pub fn run(
        &self,
        event: HookEvent,
        subject: &str,
        input: Value,
        workspace: &Workspace,
    ) -> Result<HookOutcome, String> {
        self.run_with_model(event, subject, input, workspace, None)
    }
    pub fn run_with_model(
        &self,
        event: HookEvent,
        subject: &str,
        input: Value,
        workspace: &Workspace,
        model: Option<(&dyn crate::model::ModelApi, &crate::model::TurnRequest)>,
    ) -> Result<HookOutcome, String> {
        self.decided(event, subject, input, workspace, model).map(HookOutcome::without_trailing_whitespace)
    }
    fn decided(
        &self,
        event: HookEvent,
        subject: &str,
        mut input: Value,
        workspace: &Workspace,
        model: Model<'_>,
    ) -> Result<HookOutcome, String> {
        let mut decision = HookOutcome::default();
        input["hook_event_name"] = json!(event);
        input["session_id"] = json!(workspace.session_id);
        input["agent_type"] = json!(workspace.agent_name);
        input["cwd"] = json!(workspace.cwd);
        for group in self.0.get(&event).into_iter().flatten() {
            let fail_closed = group.hooks.iter().all(|handler| handler.fail_closed);
            match group.applies_to(subject).map_err(|error| failing::errored(event, fail_closed, error)) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(failing::Failed::GoesOn(error)) => {
                    tracing::warn!(event = event.as_str(), %error, "a hook matcher failed; its group was skipped");
                    continue;
                }
                Err(failing::Failed::Stops(error)) => return Err(error),
                Err(failing::Failed::Blocks(why)) => {
                    decision.blocked = Some(why);
                    return Ok(decision);
                }
            }
            for handler in &group.hooks {
                let result = match handler.runs(&input, workspace, model) {
                    Ok(result) => result,
                    Err(error) => match failing::errored(event, handler.fail_closed, error) {
                        failing::Failed::Stops(error) => return Err(error),
                        failing::Failed::Blocks(why) => {
                            decision.blocked = Some(why);
                            return Ok(decision);
                        }
                        failing::Failed::GoesOn(error) => {
                            rewrites::failed(event, &error, &mut decision);
                            continue;
                        }
                    },
                };
                if absorbed(event, handler.fail_closed, result, &mut decision, &mut input)? == Handled::Blocked {
                    return Ok(decision);
                }
            }
        }
        Ok(decision)
    }
}

#[cfg(test)]
mod tests;
