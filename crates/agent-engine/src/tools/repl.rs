use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use domain::code_mode::Declared;
use platform::sync::Locked;
use serde_json::Value;

use super::Tool;
use crate::model::ToolSpec;

pub use domain::code_mode::{DESCRIBE as DESCRIBE_TOOL, READ_TEXT, SEARCH as SEARCH_TOOLS, TOOL as REPL_TOOL};

const KEPT: usize = 8;
const IDLE: Duration = Duration::from_secs(600);
const HELPERS: &str = "repl/helpers.ts";

pub const OUT_OF_REACH: [&str; 2] = [REPL_TOOL, "ExitPlanMode"];

pub const TOP_LEVEL: [&str; 2] = [REPL_TOOL, "ExitPlanMode"];

pub fn tool() -> Option<Box<dyn Tool>> {
    super::script_tool::shipped(REPL_TOOL).map(|tool| Box::new(tool) as Box<dyn Tool>)
}

#[derive(Debug)]
pub struct Context {
    pub session: Arc<script::session::Session>,
    pub started_now: bool,
}

#[derive(Debug, Default)]
struct Contexts(Vec<(String, Arc<script::session::Session>, Instant)>);

impl Contexts {
    fn get(&mut self, id: &str, now: Instant, start: impl FnOnce() -> script::session::Session) -> Context {
        if let Some(at) = self.0.iter().position(|(owner, ..)| owner == id) {
            let (owner, session, _) = self.0.remove(at);
            self.0.push((owner, Arc::clone(&session), now));
            return Context { session, started_now: false };
        }
        let session = Arc::new(start());
        self.0.push((id.to_owned(), Arc::clone(&session), now));
        while self.0.len() > KEPT && self.0.first().is_some_and(|(.., used)| now.duration_since(*used) >= IDLE) {
            self.0.remove(0);
        }
        Context { session, started_now: true }
    }
}

pub fn session(id: &str) -> Context {
    static ALL: OnceLock<Mutex<Contexts>> = OnceLock::new();
    let mut all = ALL.get_or_init(Mutex::default).locked();
    all.get(id, Instant::now(), || script::session::Session::with_helpers(helpers()))
}

pub fn helpers() -> String {
    content::source(HELPERS).unwrap_or_default()
}

pub fn reachable(name: &str) -> bool {
    !OUT_OF_REACH.contains(&name)
}

pub fn within_reach(offered: &[ToolSpec]) -> Vec<String> {
    let mut names: Vec<String> = offered.iter().map(|spec| spec.name.clone()).collect();
    for spec in offered.iter().filter(|spec| spec.name == REPL_TOOL) {
        for line in spec.description.lines() {
            let declared = line.strip_prefix("declare function ").and_then(|rest| rest.split('(').next());
            let declared = declared.map(|name| name.trim_start_matches("tools[\"").trim_end_matches("\"]"));
            names.extend(declared.map(str::to_owned));
        }
    }
    names
}

pub fn only(offered: Vec<ToolSpec>, more_to_find: bool) -> Vec<ToolSpec> {
    if !offered.iter().any(|spec| spec.name == REPL_TOOL) {
        return offered;
    }
    let (kept, scripted): (Vec<ToolSpec>, Vec<ToolSpec>) =
        offered.into_iter().partition(|spec| TOP_LEVEL.contains(&spec.name.as_str()));
    let declared: Vec<Declared<'_>> = scripted
        .iter()
        .map(|spec| Declared { name: &spec.name, description: &spec.description, schema: &spec.schema })
        .collect();
    let catalog = format!("{}\n\n{}", domain::code_mode::RESULT_TYPES, domain::code_mode::declarations(&declared));
    let introduction = content::render("repl/only.ts", &content::with_brand(&Value::Null));
    let finding = if more_to_find {
        format!("\n\n{}", content::render("repl/search.ts", &content::with_brand(&Value::Null)))
    } else {
        String::new()
    };
    kept.into_iter()
        .map(|mut spec| {
            if spec.name == REPL_TOOL {
                spec.description = format!("{}\n\n{introduction}{finding}\n\n```ts\n{catalog}\n```", spec.description);
            }
            spec
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;

    fn spec(name: &str) -> ToolSpec {
        ToolSpec { name: name.into(), description: format!("{name} does things."), schema: json!({ "type": "object" }) }
    }

    #[test]
    fn the_summary_is_the_description_or_else_the_first_line_of_code() {
        let repl = tool().expect("REPL ships");
        assert!(repl.script().is_some() && repl.serial(), "REPL is a script tool run one at a time");
        assert_eq!(repl.summary(&json!({ "code": "x", "description": " Count TODOs " })), "Count TODOs");
        assert_eq!(repl.summary(&json!({ "code": "const a = 1\nreturn a" })), "const a = 1");
        let mut workspace = crate::tools::Workspace::new("/tmp");
        workspace.policy.code_mode = domain::code_mode::Mode::Off;
        assert!(!repl.offered(&workspace));
        workspace.policy.code_mode = domain::code_mode::Mode::Only;
        assert!(repl.offered(&workspace));
    }

    #[test]
    fn one_session_keeps_one_context_and_only_long_idle_ones_are_let_go() {
        let mut contexts = Contexts::default();
        let start = Instant::now();
        let first = contexts.get("a", start, script::session::Session::start);
        assert!(first.started_now);
        let again = contexts.get("a", start, script::session::Session::start);
        assert!(Arc::ptr_eq(&first.session, &again.session) && !again.started_now);
        for n in 0..KEPT {
            contexts.get(&format!("other-{n}"), start, script::session::Session::start);
        }
        let busy = contexts.get("a", start, script::session::Session::start);
        assert!(Arc::ptr_eq(&first.session, &busy.session), "a context in use is never let go, however many there are");
        let later = start + IDLE;
        for n in 0..KEPT {
            contexts.get(&format!("later-{n}"), later, script::session::Session::start);
        }
        let renewed = contexts.get("a", later, script::session::Session::start);
        assert!(!Arc::ptr_eq(&first.session, &renewed.session), "past the limit, a long idle context is let go");
        assert!(renewed.started_now);
    }

    #[test]
    fn a_script_cannot_reach_the_tools_that_change_the_turn_itself() {
        assert!(!reachable("REPL") && !reachable("ExitPlanMode"));
        assert!(reachable("Read") && reachable("mcp__notes__find") && reachable("AskUserQuestion"));
    }

    #[test]
    fn in_code_only_mode_the_scripted_tools_move_into_the_repl_description() {
        let offered = vec![spec("Read"), spec(REPL_TOOL), spec("AskUserQuestion"), spec("Bash")];

        let shown = only(offered, false);

        let names: Vec<&str> = shown.iter().map(|spec| spec.name.as_str()).collect();
        assert_eq!(names, [REPL_TOOL]);
        let repl = &shown[0].description;
        assert!(repl.starts_with("REPL does things."), "{repl}");
        assert!(repl.contains("declare function Bash(") && repl.contains("declare function Read("), "{repl}");
        assert!(repl.contains("declare function AskUserQuestion("), "{repl}");
        assert!(!repl.contains("searchTools"), "nothing more to find: {repl}");
        assert!(only(vec![spec("Read"), spec(REPL_TOOL)], true)[0].description.contains("searchTools"));
    }

    #[test]
    fn the_tools_a_model_can_reach_are_its_own_and_those_its_repl_declares() {
        let offered = vec![spec("Read"), spec(REPL_TOOL), spec("has-dash"), spec("mcp__jira__ping")];
        let shown = only(offered, false);

        let reached = within_reach(&shown);

        for name in [REPL_TOOL, "Read", "has-dash", "mcp__jira__ping"] {
            assert!(reached.contains(&name.to_owned()), "{name} in {reached:?}");
        }
        assert_eq!(within_reach(&[spec("Bash")]), ["Bash"]);
    }

    #[test]
    fn without_the_repl_the_tools_stay_as_they_are() {
        let offered = vec![spec("Read"), spec("Bash")];

        assert_eq!(only(offered.clone(), true), offered);
    }

    #[test]
    fn the_same_tools_always_give_the_same_description() {
        let first = only(vec![spec("Read"), spec(REPL_TOOL), spec("Bash")], false);
        let again = only(vec![spec("Bash"), spec(REPL_TOOL), spec("Read")], false);

        assert_eq!(first[0].description, again[0].description, "a stable order keeps the prompt cache");
    }
}
