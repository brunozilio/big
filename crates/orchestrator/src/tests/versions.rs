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
