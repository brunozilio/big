use serde_json::{Value, json};

pub const FOLDER: &str = "scripts/";
const SEARCH: &str = "repl/bank.ts";

#[derive(Debug, Clone, PartialEq)]
pub struct Banked {
    pub name: String,
    pub path: String,
    pub spec: Value,
}

impl Banked {
    pub fn tools(&self) -> Vec<String> {
        self.spec["tools"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
    }

    pub fn source(&self) -> Result<String, String> {
        content::source(&self.path).ok_or_else(|| format!("{} cannot be read", self.path))
    }
}

pub fn in_bank(path: &str) -> bool {
    path.strip_prefix(FOLDER)
        .is_some_and(|rest| !rest.contains('/') && (rest.ends_with(".ts") || rest.ends_with(".js")))
}

fn stem(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_owned()
}

pub fn listed() -> Vec<Banked> {
    content::listed(FOLDER)
        .into_iter()
        .filter(|path| in_bank(path))
        .filter_map(|path| {
            let spec = content::value(&path, "spec").filter(Value::is_object)?;
            let named = spec["name"].as_str().map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned);
            Some(Banked { name: named.unwrap_or_else(|| stem(&path)), path, spec })
        })
        .collect()
}

pub fn named(name: &str) -> Result<Banked, String> {
    listed().into_iter().find(|script| script.name == name).ok_or_else(|| {
        format!("no script named {name} is in the bank; searchScripts(words) finds them by what they do")
    })
}

pub fn search(query: &str, has: impl Fn(&str) -> bool) -> Result<Value, String> {
    let scripts: Vec<Value> = listed()
        .iter()
        .map(|script| {
            let tools = script.tools();
            let missing: Vec<&String> = tools.iter().filter(|tool| !has(tool)).collect();
            json!({
                "name": script.name, "description": script.spec["description"], "input": script.spec["input"],
                "tools": tools, "missing": missing,
            })
        })
        .collect();
    content::call(SEARCH, "search", &json!({ "scripts": scripts, "query": query }))
        .ok_or_else(|| format!("{SEARCH} did not answer the search"))
}

pub fn usable(script: &Banked, has: impl Fn(&str) -> bool) -> Result<(), String> {
    let missing: Vec<String> = script.tools().into_iter().filter(|tool| !has(tool)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!("the script {} calls {}, which this agent cannot use", script.name, missing.join(", ")))
}

pub fn load(name: &str, has: impl Fn(&str) -> bool) -> Result<Value, String> {
    let script = named(name)?;
    usable(&script, has)?;
    let code = script::bank::callable(&script.path, &script.source()?)?;
    Ok(json!({ "name": script.name, "code": code }))
}

const ADAPTED: &str = "scripts/adapted.ts";

pub fn propose(path: &str, source: &str) -> Result<(), String> {
    let inside = path.strip_prefix(domain::learning::PROPOSED).is_some_and(|name| !name.contains('/'));
    if !inside || !path.ends_with(".ts") {
        return Err(format!("{path} is not a proposed script"));
    }
    script::bank::callable(path, source)?;
    content::editing_root().ok_or("this installation keeps no content folder to write the script in")?;
    content::write(path, &format!("{}\n", source.trim_end())).map_err(|error| error.to_string())
}

pub fn compile(source: &str, has: impl Fn(&str) -> bool) -> Result<Value, String> {
    let code = script::bank::callable(ADAPTED, source)?;
    let spec = script::exported(ADAPTED, source, "spec").unwrap_or(Value::Null);
    let name = spec["name"].as_str().unwrap_or("adapted").to_owned();
    usable(&Banked { name: name.clone(), path: ADAPTED.to_owned(), spec }, has)?;
    Ok(json!({ "name": name, "code": code }))
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
        content::call(SEARCH, "adapting", &asked).ok_or_else(|| format!("{SEARCH} did not answer adapting"))?;
    let answer = ask(text(&request["system"]), text(&request["prompt"]))?;
    let adapted = content::call(SEARCH, "adapted", &json!({ "answer": answer }))
        .ok_or_else(|| format!("{SEARCH} did not answer adapted"))?;
    let source = text(&adapted["source"]);
    if let Err(error) = script::bank::callable(ADAPTED, &source) {
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
            let loaded = load(&script.name, |_| true).expect("loads");
            assert!(loaded["code"].as_str().is_some_and(|code| code.contains("return {")), "{loaded}");
        }
        assert!(in_bank("scripts/x.ts") && !in_bank("scripts/proposed/x.ts") && !in_bank("agents/x.ts"));
    }

    #[test]
    fn a_search_finds_scripts_by_what_they_do_and_says_which_tools_are_missing() {
        let found = search("count matches per folder", |tool| tool != "Grep").expect("search");
        let first = &found[0];
        assert_eq!(first["name"], "count-matches");
        assert_eq!(first["missing"], json!(["Grep"]));
        assert_eq!(search("select:branch-changes", |_| true).expect("select")[0]["name"], "branch-changes");
        assert!(load("count-matches", |tool| tool != "Grep").is_err_and(|why| why.contains("Grep")));
        assert!(load("nothing-like-this", |_| true).is_err());
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
        assert_eq!(compile(source, |_| true).expect("compiles")["name"], "commits");
        assert!(compile(source, |tool| tool != "Bash").is_err());
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
                            load(call.input["name"].as_str().unwrap_or_default(), |_| true)
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
