use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{ModelApi, TurnRequest};
use crate::tools::Workspace;
use crate::tools::script_bridge::{ask_model, hosted, read_only_tool};
use crate::tools::script_io::StandaloneHost;
use crate::tools::script_tool::Origin;

mod gather;
mod inline;

pub use gather::{of_tool, tool_named};
pub use inline::{Entry, entries};

pub const MOST_NEXT: usize = 16;

const LAYER: &str = "intercept:layer";

pub type Model<'a> = Option<(&'a dyn ModelApi, &'a TurnRequest)>;

pub type Terminal<'t> = dyn FnMut(Value) -> Result<Value, String> + 't;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Tool,
    Model,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    #[default]
    Global,
    Agent,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interceptor {
    #[serde(default)]
    pub matcher: String,
    pub run: String,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
    #[serde(skip)]
    pub level: Level,
}

impl Interceptor {
    pub fn new(run: impl Into<String>, matcher: impl Into<String>, timeout: u64, level: Level) -> Self {
        Self { matcher: matcher.into(), run: run.into(), timeout, level }
    }
}

pub(crate) fn default_timeout() -> u64 {
    60
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interceptors {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool: Vec<Interceptor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model: Vec<Interceptor>,
}

pub struct Chain<'a, 't> {
    pub kind: Kind,
    pub subject: &'a str,
    pub workspace: &'a Workspace,
    pub model: Model<'a>,
    pub terminal: &'a mut Terminal<'t>,
}

impl std::fmt::Debug for Chain<'_, '_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Chain")
            .field("kind", &self.kind)
            .field("subject", &self.subject)
            .finish_non_exhaustive()
    }
}

impl Interceptors {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let parsed: Self = serde_json::from_value(value.clone()).map_err(|error| format!("intercept: {error}"))?;
        for one in parsed.tool.iter().chain(&parsed.model) {
            one.checked()?;
        }
        Ok(parsed)
    }

    #[must_use]
    pub fn at(mut self, level: Level) -> Self {
        for one in self.tool.iter_mut().chain(&mut self.model) {
            one.level = level;
        }
        self
    }

    pub fn listed(&self) -> Vec<domain::permission::InterceptorInfo> {
        let shown = |kind: &str, one: &Interceptor| domain::permission::InterceptorInfo {
            kind: kind.to_owned(),
            matcher: one.matcher.clone(),
            runs: one.run.clone(),
        };
        let tools = self.tool.iter().map(|one| shown("tool", one));
        tools.chain(self.model.iter().map(|one| shown("model", one))).collect()
    }

    pub fn extend(&mut self, other: &Self) {
        self.tool.extend(other.tool.iter().cloned());
        self.model.extend(other.model.iter().cloned());
    }

    pub fn is_empty(&self) -> bool {
        self.tool.is_empty() && self.model.is_empty()
    }

    pub fn listens_to(&self, kind: Kind) -> bool {
        !self.of(kind).is_empty()
    }

    fn of(&self, kind: Kind) -> &[Interceptor] {
        match kind {
            Kind::Tool => &self.tool,
            Kind::Model => &self.model,
        }
    }

    pub fn run(&self, value: Value, chain: Chain<'_, '_>) -> Result<Value, String> {
        let mut layers = Vec::new();
        for one in self.of(chain.kind) {
            if one.applies_to(chain.subject)? {
                layers.push(one);
            }
        }
        layers.sort_by_key(|one| one.level);
        layered(&layers, value, chain.workspace, chain.model, chain.terminal)
    }
}

impl Interceptor {
    fn checked(&self) -> Result<(), String> {
        if self.run.trim().is_empty() {
            return Err("intercept: each interceptor needs a `run`".into());
        }
        if !crate::hooks::TIMEOUT_SECONDS.contains(&self.timeout) {
            return Err(format!("intercept {}: timeout {} is outside 1..600 seconds", self.run, self.timeout));
        }
        self.applies_to("").map(|_| ()).map_err(|error| format!("intercept {}: {error}", self.run))
    }

    fn applies_to(&self, subject: &str) -> Result<bool, String> {
        if self.matcher.is_empty() || self.matcher == "*" {
            return Ok(true);
        }
        let pattern = regex::Regex::new(&format!("^(?:{})$", self.matcher))
            .map_err(|error| format!("matcher {:?} is not a regex: {error}", self.matcher))?;
        Ok(pattern.is_match(subject))
    }
}

fn layer_source(file: &str, export: &str, workspace: &Workspace) -> String {
    let data = json!({ "cwd": workspace.cwd, "session": workspace.session_id, "agent": workspace.agent_name });
    let path: Vec<&str> = export.split('.').collect();
    format!(
        r#"import * as layer from {file};
const DATA = {data};
const PATH = {path};
export default function (value, ctx) {{
  Object.assign(ctx, DATA);
  let run = layer;
  for (const key of PATH) run = run == null ? undefined : run[key];
  if (typeof run !== "function") throw new Error(`${{PATH.join(".")}} is not a function`);
  return run(value, ctx, (changed) => ctx.call("next", changed === undefined ? value : changed));
}}
"#,
        file = json!(file),
        path = json!(path),
    )
}

fn layered(
    layers: &[&Interceptor],
    value: Value,
    workspace: &Workspace,
    model: Model<'_>,
    terminal: &mut Terminal<'_>,
) -> Result<Value, String> {
    let Some((first, rest)) = layers.split_first() else { return terminal(value) };
    let (file, export) = crate::hooks::script_target(&first.run);
    let source = crate::hooks::script_source(file)?;
    let modules = [
        (file.to_owned(), script::typescript::plain(file, &source)?),
        (LAYER.to_owned(), layer_source(file, export, workspace)),
    ];
    let origin = Origin::of_script(file, workspace.home.as_deref());
    let standalone = StandaloneHost {
        cwd: workspace.cwd.clone(),
        access: workspace.policy.scripts.of(origin),
        origin,
        starter: workspace.starter(),
        stop: workspace.stop.flag(),
        cancel: workspace.stop.token().clone(),
        script: format!("the interceptor {}", first.run),
        capabilities: crate::hooks::declared(file, &source)?,
    };
    let stop = workspace.stop.flag();
    let limit = script::Limit::Busy(Duration::from_secs(first.timeout));
    let run = move |host| script::run_modules(&modules, LAYER, "default", &[value], host, stop, limit);
    let mut calls = 0;
    let ran = hosted("the interceptor stopped unexpectedly", run, |function, arguments| match function {
        "next" => {
            calls += 1;
            if calls > MOST_NEXT {
                return Err(format!("next was called more than {MOST_NEXT} times"));
            }
            layered(rest, arguments, workspace, model, &mut *terminal)
        }
        "llm" => match model {
            Some((api, template)) => ask_model(api, template, &arguments, &workspace.stop, Some(domain::origin::Origin::Hook)),
            None => Err("ctx.llm is not available here".into()),
        },
        "tool" => read_only_tool(&arguments, workspace),
        other => script::Host::call(&standalone, other, arguments),
    });
    ran.map_err(|error| format!("{}: {error}", first.run))
}

#[cfg(test)]
mod tests;
