use serde_json::{Value, json};

use crate::activation::Script;

const KEPT: [&str; 3] = ["head", "base", "method"];

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
        "kind": repository["kind"],
        "repo": repository["bindings"]["code"]["value"],
        "directory": repository["directory"],
    })
}

fn kept(input: &mut Value, key: &str, found: &Value) {
    if !input["git"].is_object() {
        input["git"] = json!({});
    }
    if !input["git"][key].is_object() {
        input["git"][key] = json!({});
    }
    for name in KEPT {
        if text(&input["git"][key][name]).is_none()
            && let Some(value) = text(&found[name])
        {
            input["git"][key][name] = json!(value);
        }
    }
    if !input["git"][key]["promote"].is_array()
        && let Some(steps) = found.get("promote").filter(|steps| steps.is_array())
    {
        input["git"][key]["promote"] = steps.clone();
    }
}

pub fn stamp(script: &Script, id: &str, input: &mut Value) {
    let repositories = input["application"]["repositories"].as_array().cloned().unwrap_or_default();
    let item = item_of(id, input);
    let application = json!({ "id": input["application"]["id"], "name": input["application"]["name"] });
    for repository in repositories {
        let Some(key) = text(&repository["id"]).map(str::to_owned) else { continue };
        let asked = json!({ "item": item, "repository": repository_of(&repository), "application": application });
        let found = match script.branch(&asked) {
            Ok(found) if found.as_object().is_some_and(|fields| !fields.is_empty()) => found,
            Ok(_) => continue,
            Err(error) => {
                tracing::warn!(%error, "the workflow's branch rule could not be read");
                continue;
            }
        };
        kept(input, &key, &found);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::resources::Resources;

    fn input(kind: &str, git: Value) -> Value {
        json!({
            "title": "Corrigir o login do App!", "issue": "OPS-7", "type": kind, "git": git,
            "application": { "id": "a", "name": "Loja", "repositories": [
                { "id": "r1", "kind": "application", "bindings": { "code": { "value": "acme/web" } } },
                { "id": "r2", "kind": "configuration", "bindings": { "code": { "value": "acme/config" } } }
            ] }
        })
    }

    fn stamped(workflow: &str, kind: &str, git: Value) -> Value {
        let resources = Resources::new(None, None, Arc::new(content::builtin_text));
        let script = Script::load(&resources, workflow).expect("workflow");
        let mut found = input(kind, git);
        stamp(&script, "0f3a9c2e", &mut found);
        found["git"].clone()
    }

    #[test]
    fn the_default_workflow_follows_gitflow_and_a_configuration_repository_goes_straight_to_its_main_branch() {
        let git = stamped("default", "Bug", Value::Null);
        let promote = json!([
            { "from": "develop", "into": "release", "method": "merge" },
            { "from": "release", "into": "main", "method": "merge" }
        ]);
        let gitflow = json!({ "head": "bugfix/OPS-7-corrigir-o-login-do-app", "base": "develop", "method": "squash", "promote": promote });
        assert_eq!(git["r1"], gitflow);
        assert_eq!(
            git["r2"],
            json!({ "head": "bugfix/OPS-7-corrigir-o-login-do-app", "method": "squash" }),
            "no base: the repository's own default branch"
        );
    }

    #[test]
    fn the_issue_type_picks_its_own_rule_and_what_the_person_chose_wins() {
        let git = stamped("default", "Hotfix", json!({ "r1": { "base": "release/2.1" } }));
        assert_eq!(git["r1"]["head"], "hotfix/OPS-7-corrigir-o-login-do-app");
        assert_eq!(git["r1"]["base"], "release/2.1");
        assert_eq!(git["r1"]["method"], "merge");
    }

    #[test]
    fn a_workflow_without_a_branch_rule_leaves_the_git_choices_alone() {
        let git = stamped("btw", "Bug", Value::Null);
        assert_eq!(git, Value::Null);
    }
}
