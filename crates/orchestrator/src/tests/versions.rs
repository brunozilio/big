use std::sync::Arc;

use crate::activation::Script;
use crate::resources::Resources;

fn repo_with(name: &str, source: &str) -> (tempfile::TempDir, Resources) {
    let repo = tempfile::tempdir().expect("repo");
    let workflows = repo.path().join("workflows");
    std::fs::create_dir_all(&workflows).expect("workflows");
    std::fs::write(workflows.join(format!("{name}.ts")), source).expect("workflow");
    let resources = Resources::new(Some(repo.path().to_path_buf()), None, Arc::new(content::builtin_text));
    (repo, resources)
}

#[test]
fn the_retired_import_of_the_release_package_still_loads() {
    let source = "import { workflow } from \"sdk\";\nimport config from \"../packages/config.ts\";\nexport default workflow({ schema_version: 1, name: \"legacy\", statuses: [config.labels.title] });\n";
    let (_repo, resources) = repo_with("legacy", source);
    let script = Script::load(&resources, "legacy").expect("the old import resolves");
    let shape = &script.described["workflows"][0];
    assert_eq!(shape["statuses"][0], "Pacote");
    assert_eq!(shape["schema_version"], 1);
}

#[test]
fn a_workflow_without_a_version_is_migrated_and_a_newer_one_is_refused() {
    let plain = "import { workflow, on } from \"sdk\";\nexport default workflow({ name: \"old\", triggers: [on(\"x.happened\")] });\n";
    let (_repo, resources) = repo_with("old", plain);
    let script = Script::load(&resources, "old").expect("migrated");
    let shape = &script.described["workflows"][0];
    assert_eq!(shape["schema_version"], content::schema::Contract::Workflow.current());
    assert_eq!(shape["triggers"][0]["schema_version"], content::schema::Contract::Trigger.current());
    let newer = "import { workflow } from \"sdk\";\nexport default workflow({ schema_version: 99, name: \"new\" });\n";
    let (_repo, resources) = repo_with("new", newer);
    let refused = Script::load(&resources, "new").err().unwrap_or_default();
    assert!(refused.contains("schema_version 99"), "{refused}");
}

#[test]
fn the_shipped_release_package_is_read_as_a_versioned_package() {
    let resources = Resources::new(None, None, Arc::new(content::builtin_text));
    let script = Script::load(&resources, "package").expect("package workflow");
    let options = &script.described["workflows"][0]["steps"][0]["options"];
    assert_eq!(options["schema_version"], content::schema::Contract::Package.current());
}

#[test]
fn the_shipped_dev_workflows_deliver_the_pull_request_kind_their_package_declares() {
    let resources = Resources::new(None, None, Arc::new(content::builtin_text));
    for name in ["default", "pull-request", "security"] {
        let script = Script::load(&resources, name).expect("workflow");
        let output = script.output.clone().expect("declared output");
        assert_eq!((output.kind.as_str(), output.via.as_deref()), ("pr", Some("code")), "{name}");
        assert_eq!(output.done.get("merged"), Some(&serde_json::json!(true)), "{name}");
    }
}

#[test]
fn a_workflow_without_output_inherits_what_its_steps_deliver_and_its_own_output_wins() {
    let inherited = "import { workflow } from \"sdk\";\nimport { develop, pr } from \"../packages/dev/steps/index.js\";\nexport default workflow({ schema_version: 1, name: \"lean\", steps: [develop({ agent: \"developer\" }), pr()] });\n";
    let (_repo, resources) = repo_with("lean", inherited);
    let script = Script::load(&resources, "lean").expect("loads");
    let output = script.output.clone().expect("inherited from the pr step");
    assert_eq!((output.kind.as_str(), output.via.as_deref()), ("pr", Some("code")));
    assert_eq!(script.described["workflows"][0]["output"]["kind"], "pr");
    let own = inherited.replace("name: \"lean\",", "name: \"lean\", output: { kind: \"report\", audience: \"me\" },");
    let (_repo, resources) = repo_with("lean", &own);
    let script = Script::load(&resources, "lean").expect("loads");
    assert_eq!(script.output.map(|output| output.kind), Some("report".to_owned()));
    let plain = "import { workflow, step } from \"sdk\";\nconst only = step({ name: \"only\", async run(ctx) { return ctx.pass(); } });\nexport default workflow({ schema_version: 1, name: \"bare\", steps: [only()] });\n";
    let (_repo, resources) = repo_with("bare", plain);
    assert_eq!(Script::load(&resources, "bare").expect("loads").output, None);
}

#[test]
fn an_output_for_an_audience_that_is_not_me_a_group_or_a_space_is_refused() {
    let source = "import { workflow } from \"sdk\";\nexport default workflow({ schema_version: 1, name: \"opinion\", output: { kind: \"report\", audience: \"everyone\" } });\n";
    let (_repo, resources) = repo_with("opinion", source);
    let refused = Script::load(&resources, "opinion").err().unwrap_or_default();
    assert!(refused.contains("audience"), "{refused}");
    let fine = source.replace("everyone", "group:legal");
    let (_repo, resources) = repo_with("opinion", &fine);
    let script = Script::load(&resources, "opinion").expect("loads");
    assert_eq!(script.output.map(|output| output.audience), Some(domain::workflow::Audience::Group("legal".into())));
}
