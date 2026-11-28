use std::path::Path;

use super::super::{ENTRY, FOLDER, ScriptedProvider};
use crate::config::Configuration;
use crate::provider::Provider;

const NEWCOMER: &str = r#"import { looks, object, read } from "sdk/connectors/contracts.ts";
export const config = {
  title: { required: true, description: "The name shown for the connector" },
  pages: { type: "integer", default: 20 },
  tags: { type: "list" },
};
export function provider(_args, ctx) {
  const name = `${ctx.config.title} ${ctx.config.pages + 1} ${(ctx.config.tags ?? []).join("+")}`;
  return { schema_version: 2, name, capabilities: ["notes"], auth: { kind: "token", fields: [] },
    actions: [read("notes.list", object([]), object([]), looks())] };
}
export function call() { return null; }
"#;

const UNDECLARED: &str = r#"import { looks, object, read } from "sdk/connectors/contracts.ts";
export function provider(_args, ctx) {
  return { schema_version: 2, name: `${ctx.config.title ?? "none"} ${typeof ctx.config.pages}`, capabilities: ["notes"],
    auth: { kind: "token", fields: [] }, actions: [read("notes.list", object([]), object([]), looks())] };
}
export function call() { return null; }
"#;

fn written(root: &Path, name: &str, source: &str) -> String {
    let dir = root.join(FOLDER).join(name);
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join(ENTRY), source).expect("index");
    format!("{FOLDER}{name}/{ENTRY}")
}

fn configuration(root: &Path, text: &str) -> Configuration {
    let file = root.join(crate::config::FILE);
    std::fs::write(&file, text).expect("config.toml");
    Configuration::built().with_file(&file)
}

fn named(root: &Path, entry: &str, configuration: &Configuration) -> Result<String, String> {
    ScriptedProvider::load_with(Some(root), entry, configuration).map(|provider| provider.descriptor().name)
}

#[test]
fn a_new_connector_reads_its_section_of_the_config_file_without_a_build() {
    let root = tempfile::tempdir().expect("root");
    let entry = written(root.path(), "newcomer", NEWCOMER);
    let read = configuration(root.path(), "[newcomer]\ntitle = \"Notes\"\ntags = \"a b\"\n");
    assert_eq!(named(root.path(), &entry, &read), Ok("Notes 21 a+b".to_owned()));
    let changed = configuration(root.path(), "[newcomer]\ntitle = \"Pad\"\npages = 4\n");
    assert_eq!(named(root.path(), &entry, &changed), Ok("Pad 5 ".to_owned()));
}

#[test]
fn a_connector_without_a_schema_keeps_reading_its_section_as_text() {
    let root = tempfile::tempdir().expect("root");
    let entry = written(root.path(), "undeclared", UNDECLARED);
    let read = configuration(root.path(), "[undeclared]\ntitle = \"Plain\"\npages = 4\n");
    assert_eq!(named(root.path(), &entry, &read), Ok("Plain string".to_owned()));
    assert_eq!(named(root.path(), &entry, &Configuration::built()), Ok("none undefined".to_owned()));
}

#[test]
fn a_section_that_breaks_the_schema_fails_only_its_connector() {
    let root = tempfile::tempdir().expect("root");
    let broken = written(root.path(), "newcomer", NEWCOMER);
    let fine = written(root.path(), "undeclared", UNDECLARED);
    let read = configuration(root.path(), "[newcomer]\npages = \"many\"\n\n[undeclared]\ntitle = \"Still\"\n");
    let refused = named(root.path(), &broken, &read).err().unwrap_or_default();
    assert!(refused.contains("the [newcomer] section of config.toml"), "{refused}");
    assert!(refused.contains("`title` is required") && refused.contains("`pages` must be a whole number"), "{refused}");
    assert_eq!(named(root.path(), &fine, &read), Ok("Still undefined".to_owned()));
}

#[test]
fn every_shipped_connector_s_section_meets_the_schema_it_declares() {
    for entry in super::super::entries() {
        ScriptedProvider::load_with(None, &entry, &Configuration::built())
            .unwrap_or_else(|error| panic!("{entry}: {error}"));
    }
}
