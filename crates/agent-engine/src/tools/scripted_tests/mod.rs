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
    assert!(shipped("Bash").profile.is_some_and(|profile| profile.strict()));
    assert!(shipped("Read").profile.is_some_and(|profile| !profile.acts()));
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
    assert_eq!(parsed.profile, None);
}
#[test]
fn a_tool_s_profile_only_tightens_what_its_call_is_asked_about() {
    use crate::tools::{Effect, Tool, Workspace};
    let workspace = Workspace::new(".");
    let tool = |name: &str, profile: &str, trusted: bool| {
        let source = format!(
            "export const spec = {{ name: \"{name}\"{profile} }};\nexport function execute() {{ return \"\"; }}\n"
        );
        super::script_tool::parse_source(std::path::Path::new("tools/T.ts"), source, trusted).expect("loads")
    };
    let operations = |tool: &ScriptTool| -> Vec<bool> {
        tool.effects(&serde_json::json!({}), &workspace)
            .iter()
            .filter_map(|effect| match effect {
                Effect::Operation(operation) => Some(operation.strict()),
                _ => None,
            })
            .collect()
    };
    let calm =
        ", profile: { effects: [\"write\"], risk: \"low\", reversible: true, untrusted: false, sensitive: false }";
    let sends = ", profile: { effects: [\"send-external\"], risk: \"medium\", reversible: false, untrusted: false, sensitive: false }";
    assert!(operations(&tool("Calm", calm, true)).is_empty());
    assert!(
        tool("Calm", calm, true).effects(&serde_json::json!({}), &workspace).iter().any(Effect::acts),
        "still asked as before"
    );
    assert_eq!(operations(&tool("Sends", sends, true)), [true]);
    assert_eq!(operations(&tool("Sends", sends, false)), [true], "a project tool can tighten too");
    assert_eq!(operations(&tool("Bare", "", true)), [true], "no profile is the riskiest");
    assert!(operations(&shipped("Read")).is_empty(), "a built-in tool keeps its own per-call effects");
}
#[test]
fn a_user_tool_without_a_profile_is_asked_about_never_refused_unless_it_only_reads() {
    use crate::tools::{Tool, Workspace};
    use domain::permission::{Bounds, Call, PolicyContext, PolicyEngine, Reason, Remembered, Verdict};
    let workspace = Workspace::new(".");
    let bounds = Bounds::of(vec![std::path::PathBuf::from("/work")], Vec::new());
    let remembered = Remembered::default();
    let cx = PolicyContext {
        mode: domain::PermissionMode::Auto,
        ceiling: None,
        remembered: &remembered,
        bounds: &bounds,
        no_prompts: false,
        allowed: None,
    };
    let judged = |spec: &str| {
        let source =
            format!("export const spec = {{ name: \"Bare\"{spec} }};\nexport function execute() {{ return \"\"; }}\n");
        let tool =
            super::script_tool::parse_source(std::path::Path::new("tools/Bare.ts"), source, true).expect("loads");
        let effects = tool.effects(&serde_json::json!({}), &workspace);
        PolicyEngine::decide(&Call { tool: "Bare", subject: "", effects: &effects }, &cx)
    };
    let bare = judged("");
    assert!(matches!(bare, Verdict::Ask { reason: Reason::Declared { .. }, rememberable: false }), "{bare:?}");
    assert_eq!(judged(", access: \"read\""), Verdict::Allow, "a tool that only reads runs as before");
}
