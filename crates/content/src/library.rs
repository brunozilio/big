use std::collections::{BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};

pub const ENTRY: &str = "library.ts";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Missing,
    ReadOnly,
    Exists,
    Invalid(String),
    Failed(String),
}

#[derive(Debug)]
pub struct Change {
    pub source: Option<String>,
    pub values: Value,
    pub body: Option<String>,
}

struct Section {
    key: String,
    folder: String,
    exports: Vec<String>,
    shown: Value,
}

struct Item {
    section: usize,
    path: String,
    files: Vec<String>,
}

struct Library {
    sections: Vec<Section>,
    items: Vec<Item>,
    paths: Vec<String>,
}

fn ask(export: &str, data: &Value) -> Result<Value, Failure> {
    super::call(ENTRY, export, data).ok_or_else(|| Failure::Failed(format!("{ENTRY} did not answer {export}")))
}

fn words(value: &Value) -> Vec<String> {
    value.as_array().into_iter().flatten().filter_map(|item| item.as_str().map(str::to_owned)).collect()
}

fn word(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

fn sections(language: &str) -> Result<(Vec<Section>, Vec<String>), Failure> {
    let declared = ask("sections", &json!({ "language": language }))?;
    let mut folders = BTreeSet::new();
    let mut found = Vec::new();
    for section in declared.as_array().into_iter().flatten() {
        let folder = word(&section["folder"]);
        if folder.is_empty() || !folder.ends_with('/') {
            continue;
        }
        folders.insert(folder.clone());
        folders.extend(words(&section["needs"]).into_iter().filter(|need| need.ends_with('/')));
        let shown = json!({
            "key": section["key"], "label": section["label"], "hint": section["hint"], "icon": section["icon"],
            "creatable": section["creatable"].as_bool().unwrap_or(false),
        });
        found.push(Section { key: word(&section["key"]), folder, exports: words(&section["exports"]), shown });
    }
    Ok((found, folders.into_iter().collect()))
}

fn library(language: &str) -> Result<Library, Failure> {
    let (sections, folders) = sections(language)?;
    let paths: Vec<String> = folders.iter().flat_map(|folder| super::listed(folder)).collect();
    let listed = ask("listing", &json!({ "paths": paths }))?;
    let mut items = Vec::new();
    for item in listed.as_array().into_iter().flatten() {
        let Some(section) = sections.iter().position(|section| section.key == word(&item["section"])) else { continue };
        let path = word(&item["path"]);
        let inside = |path: &String| path.starts_with(&sections[section].folder);
        if inside(&path) {
            items.push(Item { section, path, files: words(&item["files"]).into_iter().filter(inside).collect() });
        }
    }
    Ok(Library { sections, items, paths })
}

fn scripted(path: &str) -> bool {
    path.ends_with(".ts") || path.ends_with(".js")
}

fn evaluated() -> &'static Mutex<HashMap<u64, Value>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Value>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn exports(section: &Section, path: &str, source: &str) -> Value {
    if !scripted(path) || section.exports.is_empty() {
        return json!({});
    }
    let mut hasher = DefaultHasher::new();
    (path, source, &section.exports).hash(&mut hasher);
    let key = hasher.finish();
    if let Some(found) = evaluated().lock().ok().and_then(|cache| cache.get(&key).cloned()) {
        return found;
    }
    let found: serde_json::Map<String, Value> = section
        .exports
        .iter()
        .map(|name| (name.clone(), script::exported(path, source, name).unwrap_or(Value::Null)))
        .collect();
    let found = Value::Object(found);
    if let Ok(mut cache) = evaluated().lock() {
        cache.insert(key, found.clone());
    }
    found
}

fn asked(section: &Section, path: &str, source: &str, paths: &[String], language: &str) -> Value {
    json!({
        "section": section.key, "path": path, "source": source, "exports": exports(section, path, source),
        "paths": paths, "language": language,
    })
}

fn card(library: &Library, item: &Item, language: &str) -> Value {
    let section = &library.sections[item.section];
    let source = super::editable(&item.path).unwrap_or_default();
    let card = ask("card", &asked(section, &item.path, &source, &[], language)).unwrap_or_default();
    json!({
        "path": item.path, "title": card["title"], "description": card["description"], "avatar": card["avatar"],
        "origin": super::origin(&item.path).name(), "files": item.files,
    })
}

pub fn listing(language: &str) -> Result<Value, Failure> {
    let library = library(language)?;
    let sections: Vec<Value> = library
        .sections
        .iter()
        .enumerate()
        .map(|(at, section)| {
            let items: Vec<Value> = library
                .items
                .iter()
                .filter(|item| item.section == at)
                .map(|item| card(&library, item, language))
                .collect();
            let mut shown = section.shown.clone();
            shown["items"] = Value::Array(items);
            shown
        })
        .collect();
    Ok(json!({ "editable": super::editing_root().is_some(), "sections": sections }))
}

fn owner<'a>(library: &'a Library, path: &str) -> Result<&'a Item, Failure> {
    library
        .items
        .iter()
        .find(|item| item.path == path || item.files.iter().any(|file| file == path))
        .ok_or(Failure::Missing)
}

fn opened(library: &Library, item: &Item, path: &str, language: &str) -> Result<Value, Failure> {
    let section = &library.sections[item.section];
    let source = super::editable(path).ok_or(Failure::Missing)?;
    let shape = ask("open", &asked(section, path, &source, &library.paths, language))?;
    let card = card(library, item, language);
    Ok(json!({
        "path": path, "item": item.path, "section": section.key, "title": card["title"], "avatar": card["avatar"],
        "origin": super::origin(path).name(), "source": source, "files": item.files,
        "mode": shape["mode"], "language": shape["language"], "fields": shape["fields"], "body": shape["body"],
        "assist": shape["assist"], "outline": shape["outline"],
        "editable": super::editing_root().is_some(),
    }))
}

pub fn open(path: &str, language: &str) -> Result<Value, Failure> {
    let library = library(language)?;
    opened(&library, owner(&library, path)?, path, language)
}

fn checked(section: &Section, main: bool, path: &str, source: &str) -> Result<(), Failure> {
    if !scripted(path) {
        return Ok(());
    }
    script::typescript::plain(path, source).map_err(Failure::Invalid)?;
    if !main {
        return Ok(());
    }
    for name in &section.exports {
        match script::exported(path, source, name) {
            Ok(Value::Null) => return Err(Failure::Invalid(format!("{path} must export {name}"))),
            Ok(_) => {}
            Err(error) => return Err(Failure::Invalid(error)),
        }
    }
    Ok(())
}

fn written(path: &str, source: &str) -> Result<(), Failure> {
    if super::editing_root().is_none() {
        return Err(Failure::ReadOnly);
    }
    super::write(path, source).map_err(|error| Failure::Failed(error.to_string()))
}

pub fn save(path: &str, change: &Change, language: &str) -> Result<Value, Failure> {
    let library = library(language)?;
    let item = owner(&library, path)?;
    let section = &library.sections[item.section];
    let current = super::editable(path).ok_or(Failure::Missing)?;
    let (source, moved) = match &change.source {
        Some(source) => (source.clone(), String::new()),
        None => {
            let mut saved = asked(section, path, &current, &library.paths, language);
            saved["values"] = change.values.clone();
            saved["body"] = json!(change.body);
            let answer = ask("save", &saved)?;
            if let Some(error) = answer["error"].as_str() {
                return Err(Failure::Invalid(error.to_owned()));
            }
            (word(&answer["source"]), word(&answer["path"]))
        }
    };
    if !moved.is_empty() && moved != path {
        return moved_to(section, (path, &moved), &source, language);
    }
    checked(section, item.path == path, path, &source)?;
    if source != current {
        written(path, &source)?;
    }
    opened(&library, item, path, language)
}

fn moved_to(section: &Section, (from, to): (&str, &str), source: &str, language: &str) -> Result<Value, Failure> {
    if !to.starts_with(section.folder.as_str()) || super::override_path(std::path::Path::new(""), to).is_none() {
        return Err(Failure::Invalid(to.to_owned()));
    }
    if super::editable(to).is_some() {
        return Err(Failure::Exists);
    }
    checked(section, true, to, source)?;
    written(to, source)?;
    super::restore(from).map_err(|error| Failure::Failed(error.to_string()))?;
    open(to, language)
}

#[derive(Debug)]
pub struct Talk {
    pub values: Value,
    pub body: Option<String>,
    pub source: Option<String>,
    pub messages: Value,
}

fn talking(path: &str, talk: &Talk, language: &str) -> Result<Value, Failure> {
    let library = library(language)?;
    let item = owner(&library, path)?;
    let section = &library.sections[item.section];
    let current = super::editable(path).ok_or(Failure::Missing)?;
    let mut asking = asked(section, path, &current, &library.paths, language);
    asking["values"] = talk.values.clone();
    asking["body"] = json!(talk.body);
    asking["draft"] = json!(talk.source);
    asking["messages"] = talk.messages.clone();
    Ok(asking)
}

pub fn assist(path: &str, talk: &Talk, language: &str) -> Result<String, Failure> {
    Ok(word(&ask("assist", &talking(path, talk, language)?)?["prompt"]))
}

pub fn outline(path: &str, source: &str, language: &str) -> Result<Value, Failure> {
    let library = library(language)?;
    let section = &library.sections[owner(&library, path)?.section];
    Ok(ask("outline", &json!({ "section": section.key, "source": source }))?["steps"].clone())
}

pub fn saying(answer: &str) -> String {
    ask("saying", &json!({ "answer": answer })).map(|said| word(&said["reply"])).unwrap_or_default()
}

pub fn assisted(path: &str, talk: &Talk, answer: &str, language: &str) -> Result<Value, Failure> {
    let mut asking = talking(path, talk, language)?;
    asking["answer"] = json!(answer);
    ask("assisted", &asking)
}

pub fn create(section: &str, name: &str, language: &str) -> Result<Value, Failure> {
    let before = library(language)?;
    let folder = &before.sections.iter().find(|found| found.key == section).ok_or(Failure::Missing)?.folder;
    let made = super::call(ENTRY, "create", &json!({ "section": section, "name": name, "language": language }))
        .ok_or_else(|| Failure::Invalid(name.to_owned()))?;
    let path = word(&made["path"]);
    if !path.starts_with(folder.as_str()) || super::override_path(std::path::Path::new(""), &path).is_none() {
        return Err(Failure::Invalid(path));
    }
    if super::editable(&path).is_some() {
        return Err(Failure::Exists);
    }
    written(&path, &word(&made["source"]))?;
    open(&path, language)
}

pub fn create_file(item: &str, name: &str, language: &str) -> Result<Value, Failure> {
    let library = library(language)?;
    let owner = library.items.iter().find(|found| found.path == item).ok_or(Failure::Missing)?;
    let folder = std::path::Path::new(item).parent().and_then(|parent| parent.to_str()).unwrap_or_default();
    let name = name.trim().trim_matches('/');
    let own_folder = format!("{folder}/") != library.sections[owner.section].folder;
    let plain = !name.is_empty()
        && !name.split('/').any(|part| part.starts_with('.'))
        && super::override_path(std::path::Path::new(""), name).is_some();
    if !own_folder || !plain {
        return Err(Failure::Invalid(name.to_owned()));
    }
    let path = format!("{folder}/{name}");
    if super::editable(&path).is_some() {
        return Err(Failure::Exists);
    }
    written(&path, "")?;
    open(&path, language)
}

pub fn restore(path: &str, language: &str) -> Result<(), Failure> {
    let library = library(language)?;
    owner(&library, path)?;
    if super::editing_root().is_none() {
        return Err(Failure::ReadOnly);
    }
    super::restore(path).map_err(|error| Failure::Failed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(export: &str, data: Value) -> Value {
        ask(export, &data).expect(export)
    }

    fn agent() -> (String, String) {
        let path = "agents/developer.ts".to_owned();
        let source = String::from_utf8(crate::builtin(&path).expect("shipped").to_vec()).expect("text");
        (path, source)
    }

    fn about(section: &str, path: &str, source: &str, exported: &[&str]) -> Value {
        let section = Section {
            key: section.into(),
            folder: String::new(),
            exports: exported.iter().map(|name| (*name).to_owned()).collect(),
            shown: Value::Null,
        };
        asked(&section, path, source, &["tools/Read.ts".into(), "skills/verify/SKILL.md".into()], "en")
    }

    #[test]
    fn every_shipped_item_is_listed_under_its_section_with_a_title() {
        let listed = listing("pt-BR").expect("listing");
        let sections = listed["sections"].as_array().expect("sections");
        let keys: Vec<&str> = sections.iter().filter_map(|section| section["key"].as_str()).collect();
        assert_eq!(keys, ["agents", "skills", "prompts", "workflows", "tools", "styles", "scripts"]);
        for section in sections {
            let items = section["items"].as_array().expect("items");
            assert!(!items.is_empty(), "{} is empty", section["key"]);
            assert!(
                items.iter().all(|item| item["title"].as_str().is_some_and(|title| !title.is_empty())),
                "{items:?}"
            );
        }
        let skill = sections[1]["items"]
            .as_array()
            .expect("skills")
            .iter()
            .find(|item| item["path"] == "skills/verify/SKILL.md");
        assert!(skill.expect("verify")["files"].as_array().is_some_and(|files| !files.is_empty()));
    }

    #[test]
    fn every_shipped_agent_opens_as_a_form_and_saves_back_to_the_same_meaning() {
        for path in crate::paths().into_iter().filter(|path| path.starts_with("agents/")) {
            let source = String::from_utf8(crate::builtin(path).expect("shipped").to_vec()).expect("text");
            let about = about("agents", path, &source, &["spec", "default"]);
            let shape = run("open", about.clone());
            assert_eq!(shape["mode"], "form", "{path}");
            let mut saved = about.clone();
            saved["values"] = json!({});
            saved["body"] = Value::Null;
            let again = word(&run("save", saved)["source"]);
            assert_eq!(script::exported(path, &again, "spec"), script::exported(path, &source, "spec"), "{path}");
            assert_eq!(script::exported(path, &again, "default"), script::exported(path, &source, "default"), "{path}");
        }
    }

    #[test]
    fn a_form_change_reaches_the_agent_s_file_and_keeps_what_the_form_does_not_show() {
        let (path, source) = agent();
        let mut saved = about("agents", &path, &source, &["spec", "default"]);
        saved["values"] = json!({ "label": "Builder", "tools": ["Read"], "avatar.color": "#000000", "model": "" });
        saved["body"] = json!("Use `code` and ${nothing} \\ here.");
        let again = word(&run("save", saved)["source"]);
        let spec = script::exported(&path, &again, "spec").expect("spec");
        assert_eq!(spec["label"], "Builder");
        assert_eq!(spec["tools"], json!(["Read"]));
        assert_eq!(spec["avatar"], json!({ "shape": "squircle", "color": "#000000" }));
        assert_eq!(spec["observer"], json!(["observer"]), "untouched");
        assert!(spec.get("model").is_none());
        assert_eq!(script::exported(&path, &again, "default").expect("default"), "Use `code` and ${nothing} \\ here.");
    }

    #[test]
    fn a_skill_keeps_its_other_front_matter_and_takes_a_long_description() {
        let source = "---\nname: mine\ndescription: |-\n  First.\n  Second.\nallowed-tools: Read\n---\n\nBody.\n";
        let about = about("skills", "skills/mine/SKILL.md", source, &[]);
        let shape = run("open", about.clone());
        assert_eq!(shape["fields"][1]["value"], "First.\nSecond.");
        assert_eq!(shape["body"]["text"], "Body.\n");
        let mut saved = about;
        saved["values"] = json!({ "name": "mine", "description": "Use it: always", "disable-model-invocation": true });
        saved["body"] = json!("New body.");
        let again = word(&run("save", saved)["source"]);
        assert_eq!(
            again,
            "---\nname: mine\ndescription: \"Use it: always\"\nallowed-tools: Read\ndisable-model-invocation: true\n---\n\nNew body.\n"
        );
    }

    #[test]
    fn a_plain_prompt_is_a_text_and_one_with_code_stays_code() {
        let plain = run("open", about("prompts", "prompts/a.js", "export default `Say \\`hi\\`.`;\n", &["default"]));
        assert_eq!(plain["mode"], "form");
        assert_eq!(plain["body"]["text"], "Say `hi`.");
        let coded = "const a = 1;\nexport default function () { return `${a}`; }\n";
        assert_eq!(run("open", about("prompts", "prompts/b.js", coded, &["default"]))["mode"], "code");
        let workflow = run(
            "card",
            about(
                "workflows",
                "workflows/default.ts",
                &String::from_utf8_lossy(crate::builtin("workflows/default.ts").expect("shipped")),
                &[],
            ),
        );
        assert!(workflow["description"].as_str().is_some_and(|steps| steps.starts_with("plan → ticket")), "{workflow}");
    }

    #[test]
    fn a_new_item_gets_a_path_from_its_name_and_broken_code_is_refused() {
        let made = run("create", json!({ "section": "agents", "name": "Revisor de Segurança", "language": "pt-BR" }));
        assert_eq!(made["path"], "agents/revisor-de-seguranca.ts");
        let source = word(&made["source"]);
        let section = Section {
            key: "agents".into(),
            folder: "agents/".into(),
            exports: vec!["spec".into(), "default".into()],
            shown: Value::Null,
        };
        assert_eq!(checked(&section, true, "agents/x.ts", &source), Ok(()));
        assert!(matches!(checked(&section, true, "agents/x.ts", "export const spec = {"), Err(Failure::Invalid(_))));
        assert!(matches!(checked(&section, true, "agents/x.ts", "export const spec = {};"), Err(Failure::Invalid(_))));
        assert!(ask("create", &json!({ "section": "prompts", "name": "x" })).is_err());
    }

    #[test]
    fn a_script_of_the_bank_shows_its_spec_and_a_new_one_is_valid() {
        for path in crate::paths().into_iter().filter(|path| path.starts_with("scripts/")) {
            let source = String::from_utf8_lossy(crate::builtin(path).expect("shipped")).into_owned();
            let card = run("card", about("scripts", path, &source, &["spec"]));
            assert_eq!(card["title"].as_str(), path.strip_prefix("scripts/").and_then(|name| name.strip_suffix(".ts")));
            assert!(card["description"].as_str().is_some_and(|text| !text.is_empty()), "{path}");
            assert_eq!(run("open", about("scripts", path, &source, &["spec"]))["mode"], "code");
        }
        let made = run("create", json!({ "section": "scripts", "name": "Contar arquivos", "language": "pt-BR" }));
        assert_eq!(made["path"], "scripts/contar-arquivos.ts");
        let section = Section {
            key: "scripts".into(),
            folder: "scripts/".into(),
            exports: vec!["spec".into()],
            shown: Value::Null,
        };
        assert_eq!(checked(&section, true, "scripts/contar-arquivos.ts", &word(&made["source"])), Ok(()));
    }

    #[test]
    fn a_proposed_script_waits_for_approval_and_approving_moves_it_into_the_bank() {
        let source =
            "export const spec = { name: \"tidy\", description: \"Tidy.\" };\nexport async function run() {}\n";
        let about = about("scripts", "scripts/proposed/tidy.ts", source, &["spec"]);
        assert!(run("card", about.clone())["description"].as_str().is_some_and(|text| text.starts_with("Proposed")));
        let shape = run("open", about.clone());
        assert_eq!((&shape["mode"], &shape["fields"][0]["key"]), (&json!("form"), &json!("approve")));
        let mut kept = about.clone();
        kept["values"] = json!({ "approve": false });
        kept["body"] = json!("edited");
        assert_eq!(run("save", kept)["path"], Value::Null);
        let mut approved = about;
        approved["values"] = json!({ "approve": true });
        approved["body"] = Value::Null;
        let saved = run("save", approved);
        assert_eq!((&saved["path"], &saved["source"]), (&json!("scripts/tidy.ts"), &json!(source)));
    }

    #[test]
    fn the_chat_request_carries_the_draft_and_the_reply_changes_only_what_fits() {
        let (path, source) = agent();
        let mut asking = about("agents", &path, &source, &["spec", "default"]);
        asking["values"] = json!({ "label": "Builder" });
        asking["body"] = json!("First line.\nSecond line.");
        asking["draft"] = Value::Null;
        asking["messages"] = json!([{ "role": "user", "text": "encurte" }]);
        let prompt = word(&run("assist", asking.clone())["prompt"]);
        assert!(prompt.contains("\"value\": \"Builder\""), "{prompt}");
        assert!(prompt.contains("First line.\nSecond line.") && prompt.contains("<person>\nencurte\n</person>"));
        asking["answer"] = json!(
            "```json\n{\"reply\": \"Feito.\", \"values\": {\"tools\": [\"Read\"], \"permissionMode\": \"nope\", \"x\": 1}, \"edits\": [{\"find\": \"Second\", \"replace\": \"Last\"}, {\"find\": \"absent\", \"replace\": \"\"}], \"text\": null}\n```"
        );
        let done = run("assisted", asking.clone());
        assert_eq!(done["values"], json!({ "tools": ["Read"] }));
        assert_eq!(done["body"], "First line.\nLast line.");
        assert_eq!(done["source"], Value::Null);
        assert!(done["reply"].as_str().is_some_and(|reply| reply.starts_with("Feito. (1 ")), "{done}");
        asking["answer"] = json!("Só uma resposta.");
        let talked = run("assisted", asking);
        assert_eq!(talked["reply"], "Só uma resposta.");
        assert_eq!(talked["body"], Value::Null);
        assert_eq!(talked["values"], json!({}));
    }

    #[test]
    fn a_code_item_takes_the_whole_new_text_as_its_source() {
        let source = String::from_utf8_lossy(crate::builtin("workflows/default.ts").expect("shipped")).into_owned();
        let mut asking = about("workflows", "workflows/default.ts", &source, &[]);
        asking["values"] = json!({});
        asking["body"] = Value::Null;
        asking["draft"] = json!("draft");
        asking["messages"] = json!([]);
        assert!(
            word(&run("assist", asking.clone())["prompt"])
                .contains("<text name=\"workflows/default.ts\" language=\"typescript\">\ndraft\n</text>")
        );
        asking["answer"] = json!("{\"reply\": \"ok\", \"text\": \"new\"}");
        let done = run("assisted", asking);
        assert_eq!((&done["source"], &done["body"]), (&json!("new"), &Value::Null));
    }

    #[test]
    fn the_reply_is_read_out_of_an_answer_still_being_written() {
        assert_eq!(saying("{\"rep"), "");
        assert_eq!(saying("```json\n{\"reply\": \"Fiz a mu"), "Fiz a mu");
        assert_eq!(saying("{\"reply\": \"linha\\nnova \\\"x\\\" \\u00e"), "linha\nnova \"x\" ");
        assert_eq!(saying("{\"reply\": \"pronto\", \"values\": {"), "pronto");
        assert_eq!(saying("Sem formato."), "Sem formato.");
    }

    #[test]
    fn a_skill_field_follows_the_size_and_shape_the_skills_format_asks_for() {
        let source = "---\nname: other\ndescription: Short.\nmetadata:\n  author: me\n---\n\nBody.\n";
        let about = about("skills", "skills/mine/SKILL.md", source, &[]);
        let shape = run("open", about.clone());
        assert_eq!(shape["mode"], "form", "a nested block does not push the skill to code");
        assert_eq!((&shape["fields"][0]["max"], &shape["fields"][1]["max"]), (&json!(64), &json!(1024)));
        assert_eq!(shape["fields"][0]["readOnly"], false);
        assert_eq!(shape["body"]["maxLines"], 500);
        let saving = |values: Value| {
            let mut saved = about.clone();
            saved["values"] = values;
            saved["body"] = Value::Null;
            run("save", saved)
        };
        assert!(saving(json!({ "description": "x".repeat(1025) }))["error"].as_str().is_some_and(|e| e.contains("1024")));
        assert!(saving(json!({ "description": " " }))["error"].is_string());
        assert!(saving(json!({ "name": "Not--valid" }))["error"].is_string());
        assert!(saving(json!({ "compatibility": "y".repeat(501) }))["error"].is_string());
        let kept = word(&saving(json!({ "name": "other", "description": "x".repeat(1024), "license": "MIT" }))["source"]);
        assert!(kept.contains("metadata:\n  author: me\n") && kept.contains("license: MIT\n"), "{kept}");
    }

    #[test]
    fn a_workflow_gives_its_steps_with_the_agent_and_where_a_failure_goes_back() {
        let shipped = |path: &str| String::from_utf8_lossy(crate::builtin(path).expect("shipped")).into_owned();
        let source = shipped("workflows/default.ts");
        let steps = run("outline", json!({ "section": "workflows", "source": source }))["steps"].clone();
        assert_eq!((&steps[0]["name"], &steps[0]["agent"]), (&json!("plan"), &json!("planner")));
        assert!(steps[0]["line"].as_u64().is_some_and(|line| source.lines().nth(line as usize - 1).is_some_and(|text| text.contains("plan("))));
        let checks = steps.as_array().expect("steps").iter().find(|step| step["name"] == "checks").expect("checks");
        assert_eq!((&checks["agent"], &checks["notes"]), (&json!("verifier"), &json!(["↩ develop ×3"])));
        assert_eq!(run("outline", json!({ "section": "agents", "source": source }))["steps"], json!([]));
        let style = run("open", about("styles", "output-styles/concise.md", &shipped("output-styles/concise.md"), &[]));
        assert_eq!((&style["mode"], &style["fields"][3]["value"]), (&json!("form"), &json!(true)));
        let tool = run("card", about("tools", "tools/Read.ts", &shipped("tools/Read.ts"), &["spec"]));
        assert_eq!(tool["title"], "Read");
    }
}
