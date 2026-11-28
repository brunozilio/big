mod files;
mod platform;
mod session;
mod shell;
mod web;

use super::script_tool::{ScriptTool, builtin};

pub(crate) fn shipped(name: &str) -> ScriptTool {
    builtin().into_iter().find(|tool| tool.spec.name == name).unwrap_or_else(|| panic!("{name} is a built-in script"))
}
#[test]
fn every_shipped_tool_declares_its_risk_profile() {
    let tools: Vec<&str> = content::paths()
        .into_iter()
        .filter(|path| path.starts_with("tools/") && path.ends_with(".ts") && !path.ends_with(".d.ts"))
        .collect();
    assert!(tools.len() > 30, "{tools:?}");
    for path in tools {
        let spec = content::value(path, "spec").unwrap_or_else(|| panic!("{path} has no spec"));
        let declared = spec.get("profile").unwrap_or_else(|| panic!("{path} declares no spec.profile"));
        if let Err(problem) = domain::operation::checked(declared) {
            panic!("{path}: spec.profile {problem}");
        }
    }
    assert!(shipped("Bash").profile.strict() && !shipped("Read").profile.acts());
}

#[test]
fn a_tool_with_a_broken_profile_does_not_load_and_one_without_is_the_riskiest() {
    let broken = "export const spec = { name: \"X\", profile: { effects: [\"fly\"], risk: \"low\", reversible: true, untrusted: false, sensitive: false } };\nexport function execute() { return \"\"; }\n";
    let refused = super::script_tool::parse_source(std::path::Path::new("tools/X.ts"), broken.into(), true)
        .err()
        .unwrap_or_default();
    assert!(refused.contains("spec.profile declares the unknown effect"), "{refused}");
    let bare = "export const spec = { name: \"Y\" };\nexport function execute() { return \"\"; }\n";
    let parsed =
        super::script_tool::parse_source(std::path::Path::new("tools/Y.ts"), bare.into(), true).expect("loads");
    assert_eq!(parsed.profile, domain::operation::Profile::conservative());
}
