use super::load_trusted as load;
use super::*;

#[test]
fn project_overrides_user_and_claude_settings_are_not_read() {
    let home = tempfile::tempdir().expect("valid test fixture");
    let repo = tempfile::tempdir().expect("valid test fixture");
    for (root, model) in [
        (home.path().join(brand::DIR), "user-small"),
        (repo.path().join(brand::DIR), "project-small"),
        (repo.path().join(".claude"), "claude-small"),
    ] {
        std::fs::create_dir_all(&root).expect("valid test fixture");
        std::fs::write(root.join("settings.json"), serde_json::json!({"smallModel":model}).to_string())
            .expect("valid test fixture");
    }
    let config = load(repo.path(), Some(home.path())).expect("valid test fixture");
    assert_eq!(config.small_model.as_deref(), Some("project-small"));
}

#[test]
fn managed_settings_in_the_home_have_the_last_word_over_the_project() {
    let home = tempfile::tempdir().expect("home");
    let repo = tempfile::tempdir().expect("repo");
    let (user, project) = (home.path().join(brand::DIR), repo.path().join(brand::DIR));
    std::fs::create_dir_all(&user).expect("user");
    std::fs::create_dir_all(&project).expect("project");
    std::fs::write(project.join("settings.local.json"), r#"{"smallModel":"project-small"}"#).expect("project");
    std::fs::write(user.join(MANAGED), r#"{"smallModel":"managed-small"}"#).expect("managed");
    let settings = load(repo.path(), Some(home.path())).expect("settings");
    assert_eq!(settings.small_model.as_deref(), Some("managed-small"));
}

#[test]
fn small_and_fallback_models_are_model_names() {
    let repo = tempfile::tempdir().expect("repo");
    let root = repo.path().join(brand::DIR);
    std::fs::create_dir_all(&root).expect("dir");
    std::fs::write(
        root.join("settings.json"),
        r#"{"smallModel":"claude-haiku-5-5","fallbackModel":"claude-sonnet-5-5"}"#,
    )
    .expect("write");
    let settings = load(repo.path(), None).expect("settings");
    assert_eq!(settings.small_model.as_deref(), Some("claude-haiku-5-5"));
    assert_eq!(settings.fallbacks, ["claude-sonnet-5-5"]);
    std::fs::write(root.join("settings.json"), r#"{"fallbackModel":["chatgpt:a","kiro:b"]}"#).expect("write");
    assert_eq!(load(repo.path(), None).expect("settings").fallbacks, ["chatgpt:a", "kiro:b"]);
    std::fs::write(root.join("settings.json"), r#"{"smallModel":"two words"}"#).expect("write");
    let skipped = load(repo.path(), None).expect("a project's file that does not read is skipped");
    assert_eq!(skipped.small_model, None);
    assert!(skipped.notices.iter().any(|notice| notice.contains("must be a model name")), "{:?}", skipped.notices);
    std::fs::write(root.join("settings.json"), r#"{"fallbackModel":["ok","two words"]}"#).expect("write");
    assert!(load(repo.path(), None).expect("skipped").fallbacks.is_empty());

    let home = tempfile::tempdir().expect("home");
    std::fs::create_dir_all(home.path().join(brand::DIR)).expect("dir");
    std::fs::write(home.path().join(brand::DIR).join("settings.json"), r#"{"smallModel":"two words"}"#).expect("write");
    assert!(load(repo.path(), Some(home.path())).is_err(), "the owner's own file still stops the run");
}

#[test]
fn any_settings_file_can_turn_a_project_s_mcp_server_off() {
    let home = tempfile::tempdir().expect("home");
    let repo = tempfile::tempdir().expect("repo");
    let write = |path: std::path::PathBuf, text: &str| {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, text).expect("write");
    };
    write(repo.path().join(concat!(brand::dir!(), "/settings.json")), r#"{"disabledMcpServers": ["jira"]}"#);
    write(home.path().join(concat!(brand::dir!(), "/settings.json")), r#"{"disabledMcpServers": ["sonar"]}"#);
    let approval = load(repo.path(), Some(home.path())).expect("settings").mcp_approval;
    assert!(approval.disabled.contains(&"jira".to_owned()) && approval.disabled.contains(&"sonar".to_owned()));
}

#[test]
fn a_checked_in_file_can_only_take_script_access_away() {
    let home = tempfile::tempdir().expect("home");
    let repo = tempfile::tempdir().expect("repo");
    let write = |root: std::path::PathBuf, name: &str, scripts: serde_json::Value| {
        std::fs::create_dir_all(&root).expect("dir");
        std::fs::write(root.join(name), serde_json::json!({ "scripts": scripts }).to_string()).expect("write");
    };
    write(home.path().join(brand::DIR), "settings.json", serde_json::json!({ "processes": false }));
    write(repo.path().join(brand::DIR), "settings.json", serde_json::json!({ "processes": true, "network": false }));
    let scripts = load(repo.path(), Some(home.path())).expect("load").scripts.owner;
    assert!(!scripts.processes, "the repository cannot turn back on what the owner turned off");
    assert!(!scripts.network && scripts.files);

    write(repo.path().join(brand::DIR), "settings.local.json", serde_json::json!({ "processes": true }));
    let scripts = load(repo.path(), Some(home.path())).expect("load").scripts.owner;
    assert!(scripts.processes, "the owner's local file may");
}

#[test]
fn a_project_s_scripts_get_nothing_until_the_owner_s_settings_turn_it_on() {
    use crate::tools::script_tool::ScriptAccess;
    let home = tempfile::tempdir().expect("home");
    let repo = tempfile::tempdir().expect("repo");
    let write = |root: std::path::PathBuf, scripts: serde_json::Value| {
        std::fs::create_dir_all(&root).expect("dir");
        std::fs::write(root.join("settings.json"), serde_json::json!({ "scripts": scripts }).to_string())
            .expect("write");
    };
    let scripts = load(repo.path(), Some(home.path())).expect("load").scripts;
    assert_eq!(scripts.project, ScriptAccess::none());
    assert_eq!(scripts.owner, ScriptAccess::default(), "the user's own scripts keep everything");

    let all = serde_json::json!({ "files": true, "processes": true, "network": true });
    write(repo.path().join(brand::DIR), all);
    let scripts = load(repo.path(), Some(home.path())).expect("load").scripts;
    assert_eq!(scripts.project, ScriptAccess::none(), "the repository cannot grant itself access");

    std::fs::remove_file(repo.path().join(brand::DIR).join("settings.json")).expect("remove");
    write(home.path().join(brand::DIR), serde_json::json!({ "files": true }));
    let scripts = load(repo.path(), Some(home.path())).expect("load").scripts;
    assert_eq!(scripts.project, ScriptAccess { files: true, processes: false, network: false });
}
