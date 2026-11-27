use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use domain::code_mode::{ADAPT_SCRIPT, LOAD_SCRIPT, READ_TEXT, SEARCH_SCRIPTS};
use script::session::{Evaluate, Limits, Message, Session};
use serde_json::{Value, json};

use super::{Tool, Workspace, bank};

struct Headless {
    tools: Vec<(String, Box<dyn Tool>)>,
    declared: Vec<String>,
    workspace: Workspace,
}

impl Headless {
    fn has(&self, name: &str) -> bool {
        self.declared.iter().any(|declared| declared == name) && self.tools.iter().any(|(tool, _)| tool == name)
    }

    fn tool(&self, name: &str, input: &Value) -> Result<Value, String> {
        if !self.has(name) {
            return Err(format!(
                "the script calls {name}, which it does not name in spec.tools; a script that needs an agent declares requires: [\"agent\"]"
            ));
        }
        let Some((_, tool)) = self.tools.iter().find(|(tool, _)| tool == name) else {
            return Err(format!("{name} is not a tool here"));
        };
        let schema = tool.spec().schema;
        let tidied = super::checked::without_empty_optionals(&schema, input);
        let input = tidied.as_ref().unwrap_or(input);
        if let Some(refusal) = super::checked::refusal(name, &schema, input) {
            return Ok(json!({ "error": refusal }));
        }
        let outcome = tool.run(input, &self.workspace);
        match (outcome.is_error, domain::code_mode::throws_on_failure(name)) {
            (false, _) => Ok(domain::code_mode::script_value(name, input, outcome.content)),
            (true, true) => Err(outcome.content),
            (true, false) => Ok(json!({ "error": outcome.content })),
        }
    }

    fn serve(&self, function: &str, input: &Value) -> Result<Value, String> {
        let has = |tool: &str| self.has(tool);
        match function {
            LOAD_SCRIPT if input["source"].is_string() => Err("a workflow runs only scripts of the bank".into()),
            LOAD_SCRIPT => bank::load(input["name"].as_str().unwrap_or_default(), bank::WORKFLOW, has),
            SEARCH_SCRIPTS => bank::search(input["query"].as_str().unwrap_or_default(), bank::WORKFLOW, has),
            ADAPT_SCRIPT => Err("a workflow step makes no model call, so it cannot adapt a script".into()),
            READ_TEXT => super::raw_text::reading(|| self.tool("Read", &json!({ "file_path": input["path"] }))),
            name => self.tool(name, input),
        }
    }
}

pub fn run(name: &str, input: &Value, cwd: &Path, stop: Arc<AtomicBool>) -> Result<Value, String> {
    let script = bank::named(name)?;
    let headless = Headless {
        tools: super::standard().into_iter().map(|tool| (tool.spec().name, tool)).collect(),
        declared: script.tools(),
        workspace: Workspace::new(cwd),
    };
    bank::usable(&script, bank::WORKFLOW, |tool| headless.has(tool))?;
    let session = Session::with_helpers(super::repl::helpers());
    let code = format!("JSON.stringify((await scripts[{}]({input})) ?? null)", json!(script.name));
    let functions = headless.declared.clone();
    let messages = session.evaluate(Evaluate { code, functions, limits: Limits::default(), stop });
    let waited = Limits::default().total + Duration::from_secs(5);
    loop {
        match messages.recv_timeout(waited) {
            Ok(Message::Call(call)) => {
                let answer = headless.serve(&call.function, &call.input);
                call.reply.send(answer);
            }
            Ok(Message::Done(evaluation)) => {
                let text = evaluation.outcome?;
                return serde_json::from_str(&text)
                    .map_err(|error| format!("the script's result is not JSON: {error}"));
            }
            Err(_) => return Err(format!("the script {name} stopped answering")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_banked_script_runs_without_an_agent_on_the_tools_it_declares() {
        let folder = tempfile::tempdir().expect("folder");
        std::fs::create_dir_all(folder.path().join("src/b")).expect("dirs");
        std::fs::write(folder.path().join("src/a.rs"), "// TODO one\n// TODO two\n").expect("a");
        std::fs::write(folder.path().join("src/b/c.rs"), "// TODO three\n").expect("c");
        let stop = Arc::new(AtomicBool::new(false));
        let input = json!({ "pattern": "TODO", "path": folder.path().display().to_string() });

        let result = run("count-matches", &input, folder.path(), Arc::clone(&stop)).expect("ran");

        assert_eq!(result["total"], 3, "{result}");
        assert_eq!(result["folders"][0], json!({ "path": "src", "count": 3 }), "{result}");
        assert!(
            run("count-matches", &json!({}), folder.path(), Arc::clone(&stop))
                .is_err_and(|why| why.contains("pattern"))
        );
        assert!(run("nothing-like-this", &input, folder.path(), stop).is_err());
    }
}
