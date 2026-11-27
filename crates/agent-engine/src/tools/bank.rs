use serde_json::{Value, json};

const MODULE: &str = "repl/bank.ts";
pub const AGENT: &str = "agent";
pub const WORKFLOW: &str = "workflow";

fn words(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Format {
    pub bank: String,
    pub proposed: String,
    pub extensions: Vec<String>,
    pub spec: String,
    pub entry: String,
    hosts: Value,
}

impl Format {
    pub fn declared() -> Result<Format, String> {
        let value = content::value(MODULE, "format").ok_or_else(|| format!("{MODULE} exports no script format"))?;
        let text = |key: &str| value[key].as_str().map(str::trim).unwrap_or_default().to_owned();
        let format = Format {
            bank: text("bank"),
            proposed: text("proposed"),
            extensions: words(&value["extensions"]),
            spec: text("spec"),
            entry: text("entry"),
            hosts: value["hosts"].clone(),
        };
        let folders = [&format.bank, &format.proposed].iter().all(|folder| folder.ends_with('/'));
        if !folders || format.extensions.is_empty() || format.spec.is_empty() || format.entry.is_empty() {
            return Err(format!(
                "{MODULE}: format needs bank and proposed folders ending in /, extensions, spec and entry"
            ));
        }
        Ok(format)
    }

    fn inside(&self, folder: &str, path: &str) -> bool {
        path.strip_prefix(folder).is_some_and(|rest| {
            !rest.contains('/') && self.extensions.iter().any(|extension| rest.ends_with(extension.as_str()))
        })
    }

    pub fn in_bank(&self, path: &str) -> bool {
        self.inside(&self.bank, path)
    }

    pub fn proposal(&self, name: &str) -> String {
        let extension = self.extensions.first().map_or(".ts", String::as_str);
        format!("{}{name}{extension}", self.proposed)
    }

    pub fn offered(&self, host: &str) -> Vec<String> {
        words(&self.hosts[host])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Banked {
    pub name: String,
    pub path: String,
    pub spec: Value,
}

impl Banked {
    pub fn tools(&self) -> Vec<String> {
        words(&self.spec["tools"])
    }

    pub fn requires(&self) -> Vec<String> {
        words(&self.spec["requires"])
    }

    pub fn source(&self) -> Result<String, String> {
        content::source(&self.path).ok_or_else(|| format!("{} cannot be read", self.path))
    }

    fn unmet(&self, format: &Format, host: &str) -> Vec<String> {
        let offered = format.offered(host);
        self.requires().into_iter().filter(|need| !offered.contains(need)).collect()
    }
}

fn described(format: &Format, path: &str, spec: &Value) -> Option<Banked> {
    let described = content::call(MODULE, "described", &json!({ "path": path, "spec": spec }))?;
    let name = described["name"].as_str().map(str::trim).filter(|name| !name.is_empty())?.to_owned();
    format.in_bank(path).then(|| Banked { name, path: path.to_owned(), spec: described })
}

pub fn listed() -> Vec<Banked> {
    let Ok(format) = Format::declared() else { return Vec::new() };
    content::listed(&format.bank)
        .into_iter()
        .filter(|path| format.in_bank(path))
        .filter_map(|path| {
            let spec = content::value(&path, &format.spec).filter(Value::is_object)?;
            described(&format, &path, &spec)
        })
        .collect()
}

pub fn named(name: &str) -> Result<Banked, String> {
    listed().into_iter().find(|script| script.name == name).ok_or_else(|| {
        format!("no script named {name} is in the bank; searchScripts(words) finds them by what they do")
    })
}

pub fn search(query: &str, host: &str, has: impl Fn(&str) -> bool) -> Result<Value, String> {
    let format = Format::declared()?;
    let scripts: Vec<Value> = listed()
        .iter()
        .map(|script| {
            let tools = script.tools();
            let mut missing: Vec<String> = tools.iter().filter(|tool| !has(tool)).cloned().collect();
            missing.extend(script.unmet(&format, host));
            json!({
                "name": script.name, "description": script.spec["description"], "input": script.spec["input"],
                "tools": tools, "requires": script.requires(), "missing": missing,
            })
        })
        .collect();
    content::call(MODULE, "search", &json!({ "scripts": scripts, "query": query }))
        .ok_or_else(|| format!("{MODULE} did not answer the search"))
}

pub fn usable(script: &Banked, host: &str, has: impl Fn(&str) -> bool) -> Result<(), String> {
    let unmet = script.unmet(&Format::declared()?, host);
    if !unmet.is_empty() {
        return Err(format!(
            "the script {} declares requires: [{}], which a {host} run does not offer; run it where that is offered",
            script.name,
            unmet.iter().map(|need| format!("\"{need}\"")).collect::<Vec<_>>().join(", ")
        ));
    }
    let missing: Vec<String> = script.tools().into_iter().filter(|tool| !has(tool)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!("the script {} calls {}, which this {host} run cannot use", script.name, missing.join(", ")))
}

pub fn load(name: &str, host: &str, has: impl Fn(&str) -> bool) -> Result<Value, String> {
    let format = Format::declared()?;
    let script = named(name)?;
    usable(&script, host, has)?;
    let code = script::bank::callable(&script.path, &script.source()?, &format.entry)?;
    Ok(json!({ "name": script.name, "code": code, "entry": format.entry }))
}

const ADAPTED: &str = "scripts/adapted.ts";

pub fn propose(name: &str, source: &str) -> Result<String, String> {
    let format = Format::declared()?;
    let path = format.proposal(name);
    if !format.inside(&format.proposed, &path) {
        return Err(format!("{name} cannot name a proposed script"));
    }
    script::bank::callable(&path, source, &format.entry)?;
    content::editing_root().ok_or("this installation keeps no content folder to write the script in")?;
    content::write(&path, &format!("{}\n", source.trim_end())).map_err(|error| error.to_string())?;
    Ok(path)
}

pub fn compile(source: &str, host: &str, has: impl Fn(&str) -> bool) -> Result<Value, String> {
    let format = Format::declared()?;
    let code = script::bank::callable(ADAPTED, source, &format.entry)?;
    let spec = script::exported(ADAPTED, source, &format.spec).unwrap_or(Value::Null);
    let named = spec["name"].as_str().unwrap_or("adapted").to_owned();
    let spec = content::call(MODULE, "described", &json!({ "path": ADAPTED, "spec": spec })).unwrap_or(spec);
    usable(&Banked { name: named.clone(), path: ADAPTED.to_owned(), spec }, host, has)?;
    Ok(json!({ "name": named, "code": code, "entry": format.entry }))
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

pub fn adapt(
    input: &Value,
    tools: &[String],
    ask: impl FnOnce(String, String) -> Result<String, String>,
) -> Result<Value, String> {
    let goal = input["goal"].as_str().map(str::trim).filter(|goal| !goal.is_empty());
    let goal =
        goal.ok_or("adaptScript needs the goal, e.g. adaptScript(\"branch-changes\", \"also list untracked files\")")?;
    let source = match input["source"].as_str() {
        Some(source) => source.to_owned(),
        None => named(input["name"].as_str().unwrap_or_default())?.source()?,
    };
    let asked = json!({ "source": source, "goal": goal, "input": input["input"], "tools": tools });
    let request =
        content::call(MODULE, "adapting", &asked).ok_or_else(|| format!("{MODULE} did not answer adapting"))?;
    let answer = ask(text(&request["system"]), text(&request["prompt"]))?;
    let adapted = content::call(MODULE, "adapted", &json!({ "answer": answer }))
        .ok_or_else(|| format!("{MODULE} did not answer adapted"))?;
    let source = text(&adapted["source"]);
    if let Err(error) = script::bank::callable(ADAPTED, &source, &Format::declared()?.entry) {
        return Err(format!("the small model's script does not compile ({error}); its answer was:\n{answer}"));
    }
    Ok(json!({ "source": source, "notes": adapted["notes"] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_scripts_are_in_the_bank_and_each_one_compiles() {
        let all = listed();
        assert!(all.iter().any(|script| script.name == "branch-changes"), "{all:?}");
        for script in &all {
            assert!(script.spec["description"].as_str().is_some_and(|text| !text.is_empty()), "{}", script.name);
            let loaded = load(&script.name, AGENT, |_| true).expect("loads");
            assert_eq!(loaded["entry"], "run");
            assert!(loaded["code"].as_str().is_some_and(|code| code.contains("return {")), "{loaded}");
        }
        let format = Format::declared().expect("format");
        assert!(format.in_bank("scripts/x.ts") && !format.in_bank("scripts/proposed/x.ts"));
        assert!(!format.in_bank("agents/x.ts") && !format.in_bank("scripts/x.md"));
        assert_eq!(format.proposal("x"), "scripts/proposed/x.ts");
        assert_eq!((format.offered(AGENT), format.offered(WORKFLOW)), (vec!["agent".to_owned()], Vec::new()));
    }

    #[test]
    fn a_search_finds_scripts_by_what_they_do_and_says_which_tools_are_missing() {
        let found = search("count matches per folder", AGENT, |tool| tool != "Grep").expect("search");
        let first = &found[0];
        assert_eq!(first["name"], "count-matches");
        assert_eq!(first["missing"], json!(["Grep"]));
        assert_eq!(search("select:branch-changes", AGENT, |_| true).expect("select")[0]["name"], "branch-changes");
        assert!(load("count-matches", AGENT, |tool| tool != "Grep").is_err_and(|why| why.contains("Grep")));
        assert!(load("nothing-like-this", AGENT, |_| true).is_err());
    }

    #[test]
    fn a_script_that_requires_an_agent_is_refused_up_front_where_none_is_offered() {
        let source = "export const spec = { name: \"tuned\", tools: [\"Bash\"], requires: [\"agent\"] };\nexport async function run() { return await adaptScript(\"branch-changes\", \"x\"); }\n";
        assert_eq!(compile(source, AGENT, |_| true).expect("an agent offers it")["name"], "tuned");
        let refused = compile(source, WORKFLOW, |_| true).expect_err("a workflow does not");
        assert!(refused.contains("requires: [\"agent\"]") && refused.contains("workflow"), "{refused}");
    }

    #[test]
    fn the_small_model_adapts_a_script_and_only_code_that_compiles_comes_back() {
        let input = json!({ "name": "branch-changes", "goal": "only the commits", "input": { "base": "main" } });
        let tools = vec!["Bash".to_owned()];
        let mut seen = String::new();
        let adapted = adapt(&input, &tools, |system, prompt| {
            seen = format!("{system}\n{prompt}");
            Ok("```ts\nexport const spec = { name: \"commits\", tools: [\"Bash\"] };\nexport async function run(input: { base?: string }) { return await $`git log`; }\n```\nKept only the log.".into())
        })
        .expect("adapted");
        assert!(
            seen.contains("<goal>\nonly the commits\n</goal>") && seen.contains("export async function run"),
            "{seen}"
        );
        assert!(adapted["source"].as_str().is_some_and(|source| source.starts_with("export const spec")), "{adapted}");
        assert_eq!(adapted["notes"], "Kept only the log.");
        let source = adapted["source"].as_str().unwrap_or_default();
        assert_eq!(compile(source, AGENT, |_| true).expect("compiles")["name"], "commits");
        assert!(compile(source, AGENT, |tool| tool != "Bash").is_err());
        assert!(adapt(&input, &tools, |_, _| Ok("I cannot.".into())).is_err());
        assert!(adapt(&json!({ "name": "branch-changes" }), &tools, |_, _| Ok(String::new())).is_err());
    }

    #[test]
    fn a_cell_runs_a_banked_script_with_the_tools_it_was_given() {
        use script::session::{Evaluate, Limits, Message, Session};
        let session = Session::with_helpers(crate::tools::repl::helpers());
        let code = "await scripts['count-matches']({ pattern: 'TODO' })".to_owned();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let messages =
            session.evaluate(Evaluate { code, functions: vec!["Grep".into()], limits: Limits::default(), stop });
        let mut asked = Vec::new();
        let done = loop {
            match messages.recv_timeout(std::time::Duration::from_secs(20)).expect("answers") {
                Message::Call(call) => {
                    asked.push(call.function.clone());
                    let answer = match call.function.as_str() {
                        domain::code_mode::LOAD_SCRIPT => {
                            load(call.input["name"].as_str().unwrap_or_default(), AGENT, |_| true)
                        }
                        "Grep" => {
                            Ok(json!({ "counts": { "src/a.rs": 2, "src/b/c.rs": 3, "top.rs": 1 }, "numFiles": 3 }))
                        }
                        other => Err(format!("{other} is not here")),
                    };
                    call.reply.send(answer);
                }
                Message::Done(evaluation) => break evaluation,
            }
        };
        assert_eq!(asked, [domain::code_mode::LOAD_SCRIPT, "Grep"]);
        let result: Value = serde_json::from_str(&done.outcome.expect("ran")).expect("json");
        assert_eq!(result["total"], 6);
        assert_eq!(result["folders"], json!([{ "path": "src", "count": 5 }, { "path": ".", "count": 1 }]));
    }
}
