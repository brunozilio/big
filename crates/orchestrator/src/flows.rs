use std::path::Path;

use serde_json::{Map, Value, json};

const FOLDER: &str = "flows/";
const EXPORT: &str = "branch";
const KEPT: [&str; 3] = ["head", "base", "method"];

fn entries(root: Option<&Path>) -> Vec<String> {
    content::listed_in(root, FOLDER).into_iter().filter(|path| path.ends_with(".ts") || path.ends_with(".js")).collect()
}

fn named(path: &str) -> &str {
    let file = path.strip_prefix(FOLDER).unwrap_or(path);
    file.strip_suffix(".ts").or_else(|| file.strip_suffix(".js")).unwrap_or(file)
}

pub fn names_in(root: Option<&Path>) -> Vec<String> {
    let mut found: Vec<String> = entries(root).iter().map(|path| named(path).to_owned()).collect();
    found.dedup();
    found
}

pub fn names() -> Vec<String> {
    names_in(content::folder().as_deref())
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|found| !found.is_empty())
}

fn item_of(id: &str, input: &Value) -> Value {
    let event = &input["event"]["data"];
    json!({
        "id": id,
        "title": text(&input["title"]).or(text(&input["text"])),
        "text": text(&input["text"]),
        "issue": text(&input["issue"]).or(text(&event["ref"])),
        "type": text(&input["type"]).or(text(&event["type"])),
    })
}

fn repository_of(repository: &Value) -> Value {
    json!({
        "id": repository["id"],
        "repo": repository["bindings"]["code"]["value"],
        "directory": repository["directory"],
    })
}

fn declared(root: Option<&Path>, paths: &[String], asked: &Value) -> Option<Map<String, Value>> {
    paths.iter().find_map(|path| {
        let found = connectors::scripted::evaluate(root, path, EXPORT, asked)
            .inspect_err(|error| tracing::warn!(%error, path, "the flow could not be read"))
            .ok()?;
        found.as_object().filter(|fields| !fields.is_empty()).cloned()
    })
}

pub fn stamp_in(root: Option<&Path>, id: &str, input: &mut Value) {
    let repositories = input["application"]["repositories"].as_array().cloned().unwrap_or_default();
    if repositories.is_empty() {
        return;
    }
    let paths = entries(root);
    if paths.is_empty() {
        return;
    }
    let item = item_of(id, input);
    let application = json!({ "id": input["application"]["id"], "name": input["application"]["name"] });
    for repository in repositories {
        let Some(key) = text(&repository["id"]).map(str::to_owned) else { continue };
        let flow = text(&repository["flow"]).unwrap_or(brand::workflow::FLOW);
        let chosen: Vec<String> = paths.iter().filter(|path| named(path) == flow).cloned().collect();
        let asked = json!({ "item": item, "repository": repository_of(&repository), "application": application });
        let Some(found) = declared(root, &chosen, &asked) else { continue };
        if !input["git"].is_object() {
            input["git"] = json!({});
        }
        if !input["git"][&key].is_object() {
            input["git"][&key] = json!({});
        }
        for name in KEPT {
            if text(&input["git"][&key][name]).is_none()
                && let Some(value) = text(&found[name])
            {
                input["git"][&key][name] = json!(value);
            }
        }
        if !input["git"][&key]["promote"].is_array()
            && let Some(steps) = found.get("promote").filter(|steps| steps.is_array())
        {
            input["git"][&key]["promote"] = steps.clone();
        }
    }
}

pub fn stamp(id: &str, input: &mut Value) {
    stamp_in(content::folder().as_deref(), id, input);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::stamp_in;

    const FLOW: &str = r#"import { flow } from "sdk/flow.js";

export const branch = flow({
  branch: "feature/{issue}-{slug}",
  base: "develop",
  method: "squash",
  byType: { Hotfix: { branch: "hotfix/{issue}", base: "main", method: "merge" } },
});
"#;

    fn input(kind: &str, git: serde_json::Value) -> serde_json::Value {
        let mut found = bare(kind, git);
        found["application"]["repositories"][0]["flow"] = json!("web");
        found
    }

    fn bare(kind: &str, git: serde_json::Value) -> serde_json::Value {
        json!({
            "title": "Corrigir o login do App!", "issue": "OPS-7", "type": kind, "git": git,
            "application": { "id": "a", "name": "Loja", "repositories": [
                { "id": "r1", "directory": "/web", "bindings": { "code": { "connection": "c", "value": "acme/web" } } },
                { "id": "r2", "directory": "/api", "bindings": { "code": { "connection": "c", "value": "acme/api" } } }
            ] }
        })
    }

    fn stamped(kind: &str, git: serde_json::Value) -> serde_json::Value {
        let root = tempfile::tempdir().expect("dir");
        std::fs::create_dir_all(root.path().join("flows")).expect("flows");
        std::fs::write(root.path().join("flows/web.ts"), FLOW).expect("written");
        let mut found = input(kind, git);
        stamp_in(Some(root.path()), "0f3a9c2e", &mut found);
        found["git"].clone()
    }

    #[test]
    fn the_flow_of_the_repository_names_the_branch_and_picks_the_base_and_the_merge() {
        let git = stamped("História", json!(null));
        assert_eq!(
            git["r1"],
            json!({ "head": "feature/OPS-7-corrigir-o-login-do-app", "base": "develop", "method": "squash" })
        );
        assert_eq!(git["r2"]["head"], "feature/OPS-7-corrigir-o-login-do-app", "a repository that names no flow follows the default one");
    }

    #[test]
    fn the_issue_type_picks_its_own_rule_and_what_the_person_chose_wins() {
        let git = stamped("Hotfix", json!({ "r1": { "base": "release/2.1" } }));
        assert_eq!(git["r1"], json!({ "head": "hotfix/OPS-7", "base": "release/2.1", "method": "merge" }));
    }

    #[test]
    fn a_repository_that_names_no_flow_follows_the_default_one() {
        let mut found = bare("Bug", json!(null));
        stamp_in(None, "0f3a9c2e", &mut found);
        let promote = json!([
            { "from": "develop", "into": "release", "method": "merge" },
            { "from": "release", "into": "main", "method": "merge" }
        ]);
        let expected = json!({ "head": "bugfix/OPS-7-corrigir-o-login-do-app", "base": "develop", "method": "squash", "promote": promote });
        assert_eq!(found["git"]["r1"], expected);
        assert_eq!(found["git"]["r2"], expected);
        assert!(super::names_in(None).contains(&brand::workflow::FLOW.to_owned()));
    }
}
