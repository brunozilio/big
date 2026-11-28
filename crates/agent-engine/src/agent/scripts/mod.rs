use std::cell::{Cell, RefCell};

use serde_json::{Value, json};

use super::Agent;
use super::dispatch::Calling;
use super::run::ThisRun;
use super::subagents::background::Spawn;
use crate::model::ToolSpec;
use crate::tools::script_bridge::hosted;
use crate::tools::script_tool::{Capabilities, Capability, Origin, ScriptTool};
use crate::tools::skill::FoundSkill;
use crate::tools::{Outcome, Tool, Workspace};
use crate::transcript::{Block, Message};

mod agents;
mod background;
mod host_calls;
mod peers;
mod plan;
mod repl;
mod skill;
mod spaces;
mod user;
mod watch;

use host_calls::HostCall;

const DEEPEST: usize = 8;

struct Script<'a> {
    name: &'a str,
    path: &'a str,
    source: &'a str,
    origin: Origin,
    capabilities: Capabilities,
    effects: Vec<crate::tools::Effect>,
    streams: crate::model::script_http::Streams,
}

#[derive(Clone, Copy)]
struct Within<'a, 'env> {
    id: &'a str,
    parent: &'a [Message],
    run: &'a ThisRun,
    spawn: Option<Spawn<'a, 'env>>,
    permission: Option<domain::permission::RuleKind>,
}

impl<'a, 'env> Within<'a, 'env> {
    fn the_call(calling: &Calling<'a, 'env>) -> Self {
        let permission = calling.permission;
        Self { id: calling.id, parent: calling.parent, run: calling.run, spawn: calling.spawn, permission }
    }

    fn permitting<'s>(&'s self, subject: &'s str, shown: Option<&'s str>) -> crate::agent::permission::Permitting<'s> {
        crate::agent::permission::Permitting {
            id: self.id,
            subject,
            shown,
            parent: self.parent,
            hook_permission: self.permission,
        }
    }
}

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct ScriptCall<'w, 'env, 'b> {
    agent: &'env Agent<'b>,
    tool: &'w ScriptTool,
    within: Within<'w, 'env>,
}

impl Tool for ScriptCall<'_, '_, '_> {
    fn spec(&self) -> ToolSpec {
        self.tool.spec.clone()
    }

    fn effects(&self, input: &Value, workspace: &Workspace) -> Vec<crate::tools::Effect> {
        self.tool.effects(input, workspace)
    }

    fn summary(&self, input: &Value) -> String {
        self.tool.summary(input)
    }

    fn run(&self, input: &Value, _workspace: &Workspace) -> Outcome {
        self.agent.run_script(self.within, self.tool, input)
    }
}

fn answered(block: Block) -> Result<Value, String> {
    match block {
        Block::ToolResult { content, is_error: false, .. } | Block::ToolResultRich { content, is_error: false, .. } => {
            Ok(Value::String(content))
        }
        Block::ToolResult { content, .. } | Block::ToolResultRich { content, .. } => Err(content),
        other => Ok(json!(format!("{other:?}"))),
    }
}

fn reported(result: Result<Value, String>, last: Option<Outcome>, stopped: bool, name: &str) -> Outcome {
    match (result, last) {
        (Ok(Value::String(text)), Some(outcome)) if text == outcome.content => outcome,
        (Err(_), Some(outcome)) if stopped => outcome,
        (Ok(value), _) => crate::tools::contract::outcome(value),
        (Err(error), _) => Outcome::error(format!("{name} failed: {error}")),
    }
}

impl Agent<'_> {
    pub(in crate::agent) fn script_call<'env>(
        &'env self,
        calling: &Calling<'_, 'env>,
        tool: &ScriptTool,
        summary: &str,
        input: &Value,
    ) -> Outcome {
        let call = ScriptCall { agent: self, tool, within: Within::the_call(calling) };
        let shown = tool.shown(input, self.workspace);
        self.permitted(&calling.permitting(summary, shown.as_deref()), &call, input)
    }

    pub(in crate::agent) fn project_skill<'t>(&self, tool: &'t dyn Tool, input: &Value) -> Option<&'t FoundSkill> {
        let name = input["skill"].as_str()?.trim_start_matches('/');
        tool.skills().iter().find(|skill| skill.name == name && skill.project_script())
    }

    pub(in crate::agent) fn skill_call<'env>(
        &'env self,
        calling: &Calling<'_, 'env>,
        tool: &dyn Tool,
        skill: &FoundSkill,
        summary: &str,
        input: &Value,
    ) -> Outcome {
        let shown = format!("{summary}, the project's script {}", skill.file.display());
        let Some(script) = tool.script() else {
            return self.permitted(&calling.permitting(summary, Some(&shown)), tool, input);
        };
        let call = ScriptCall { agent: self, tool: script, within: Within::the_call(calling) };
        self.permitted(&calling.permitting(summary, Some(&shown)), &call, input)
    }

    fn script_context(&self) -> Value {
        let skills: Vec<Value> = self
            .tools
            .iter()
            .flat_map(|tool| tool.skills())
            .map(|skill| json!({ "name": skill.name, "description": skill.description, "path": skill.file.display().to_string() }))
            .collect();
        json!({
            "cwd": self.workspace.cwd.display().to_string(),
            "model": self.request.model,
            "mode": format!("{:?}", self.control.mode()).to_ascii_lowercase(),
            "session": self.workspace.session_id,
            "tools": self.tools.iter().map(|tool| tool.spec().name).collect::<Vec<_>>(),
            "skills": skills,
            "agents": self.agents.iter().map(|agent| json!({ "name": agent.name, "description": agent.description, "background": agent.config.background })).collect::<Vec<_>>(),
        })
    }

    fn run_script<'env>(&'env self, within: Within<'_, 'env>, tool: &ScriptTool, input: &Value) -> Outcome {
        let path = tool.path.display().to_string();
        let origin = Origin::trusted(tool.trusted);
        let capabilities = tool.capabilities.clone();
        let effects = tool.effects(input, self.workspace);
        let streams = crate::model::script_http::Streams::default();
        let script =
            Script { name: &tool.spec.name, path: &path, source: &tool.source, origin, capabilities, effects, streams };
        self.run_js(within, &script, input)
    }

    fn run_js<'env>(&'env self, within: Within<'_, 'env>, script: &Script<'_>, input: &Value) -> Outcome {
        let depth = DEPTH.with(Cell::get);
        if depth >= DEEPEST {
            return Outcome::error(format!("{} called tools {DEEPEST} levels deep; stopped", script.name));
        }
        let data = self.script_context();
        let stop = self.workspace.stop.flag();
        let export = if script::exports(script.path, script.source, "execute") { "execute" } else { "run" };
        let (path, source, input) = (script.path.to_owned(), script.source.to_owned(), input.clone());
        let last: RefCell<Option<Outcome>> = RefCell::new(None);
        DEPTH.with(|cell| cell.set(depth + 1));
        let result = hosted(
            "the script crashed",
            move |host| script::run(&path, &source, export, &[input], &data, host, stop),
            |function, arguments| kept(self.serve(within, script, function, arguments), function, &last),
        );
        DEPTH.with(|cell| cell.set(depth));
        let stopped = self.workspace.stop.load(std::sync::atomic::Ordering::Relaxed);
        reported(result, last.into_inner(), stopped, script.name)
    }

    fn serve<'env>(
        &'env self,
        within: Within<'_, 'env>,
        script: &Script<'_>,
        function: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        if let Some(capability) = Capability::of(function) {
            return self.host_call(within, script, capability, &arguments);
        }
        if crate::tools::host::named(function) {
            return self.primitive(script, function, &arguments);
        }
        if let Some(answer) = self.loop_space(within, script, function, &arguments) {
            return answer;
        }
        match function {
            "tool" => {
                let name = arguments.get("name").and_then(Value::as_str).unwrap_or_default();
                let id = format!("script-{}", self.control.next_agent());
                let spawn = |job: Box<dyn FnOnce() + Send + '_>| job();
                answered(self.call(&id, name, &arguments["input"], within.run, within.parent, &spawn))
            }
            "llm" => self.script_llm(&arguments),
            "text" => Ok(Value::String(content::text(arguments.as_str().unwrap_or_default()))),
            "log" => {
                let text = arguments.as_str().map_or_else(|| arguments.to_string(), str::to_owned);
                (self.emit)(domain::EngineEvent::ToolLog { id: within.id.to_owned(), text });
                Ok(Value::Null)
            }
            other => Err(format!("ctx.{other} is not a function")),
        }
    }

    fn host_call(
        &self,
        within: Within<'_, '_>,
        script: &Script<'_>,
        capability: Capability,
        arguments: &Value,
    ) -> Result<Value, String> {
        script.capabilities.allows(capability, script.name)?;
        capability.granted(self.workspace.policy.scripts.of(script.origin))?;
        let call = HostCall { capability, script: script.name, origin: script.origin };
        let id = format!("{}-{}-{}", within.id, capability.name(), self.control.next_agent());
        let (subject, shown) = (call.subject(arguments), call.shown(arguments));
        let permitting = crate::agent::permission::Permitting {
            id: &id,
            subject: &subject,
            shown: Some(&shown),
            parent: within.parent,
            hook_permission: None,
        };
        HostCall::answer(self.permitted(&permitting, &call, arguments))
    }

    fn primitive(&self, script: &Script<'_>, function: &str, arguments: &Value) -> Result<Value, String> {
        if script.origin != Origin::Owner {
            return Err(format!(
                "ctx.{function} is for the owner's tools; a project tool reaches the app through readFile, writeFile, \
                 exec and fetch"
            ));
        }
        let fence = crate::tools::host::Fence::of(&script.effects);
        let reach = crate::tools::host::Reach { workspace: self.workspace, fence, streams: Some(&script.streams) };
        crate::tools::host::call(&reach, function, arguments)
    }

    fn script_llm(&self, arguments: &Value) -> Result<Value, String> {
        crate::tools::script_bridge::ask_model(self.api, &self.request, arguments, self.control.stop_flag(), None)
    }
}

fn kept(answer: Result<Value, String>, function: &str, last: &RefCell<Option<Outcome>>) -> Result<Value, String> {
    if let Ok(value) = &answer
        && spaces::named(function).is_some()
        && value.get("content").is_some()
    {
        last.replace(Some(crate::tools::contract::outcome(value.clone())));
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_group_of_primitives_hides_behind_a_field_of_the_context() {
        let fields =
            ["cwd", "model", "mode", "session", "tools", "skills", "agents", "brand", "services", "codeMode", "shell"];
        for field in fields {
            let function = format!("{field}.anything");
            assert!(!crate::tools::host::named(&function), "ctx.{field} hides a primitive group");
            assert!(spaces::named(&function).is_none(), "ctx.{field} hides a loop primitive group");
        }
        let context = ["cwd", "model", "mode", "session", "tools", "skills", "agents"];
        assert!(context.iter().all(|field| fields.contains(field)));
    }

    #[test]
    fn a_script_s_answer_keeps_a_primitive_s_outcome_when_it_passed_it_through() {
        let built = Outcome::ok("listing").showing("3 files");
        let same = reported(Ok(json!("listing")), Some(built.clone()), false, "Glob");
        assert_eq!(same, built, "what the primitive showed survives the script");
        let rewritten = reported(Ok(json!("2 files")), Some(built.clone()), false, "Glob");
        assert_eq!(rewritten, Outcome::ok("2 files"));
        assert_eq!(reported(Ok(Value::Null), None, false, "Glob"), Outcome::ok(""));
        assert_eq!(reported(Ok(json!({"n": 1})), None, false, "Glob"), Outcome::ok(r#"{"n":1}"#));
        assert_eq!(reported(Err("boom".into()), Some(built.clone()), true, "Glob"), built, "stopped mid-way");
        assert_eq!(reported(Err("boom".into()), Some(built), false, "Glob"), Outcome::error("Glob failed: boom"));
    }
}
