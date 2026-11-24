use serde_json::json;

use super::*;

fn kept_aside(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(folder)
        .expect("list")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().is_some_and(|name| name.to_string_lossy().starts_with("settings.json.corrupt-"))
        })
        .collect()
}

#[test]
fn settings_kept_in_memory_are_read_back() {
    let store = InMemorySettingsStore::default();
    assert_eq!(store.load(), None);
    store.save(&json!({"theme": "dark"})).expect("save");
    assert_eq!(store.load(), Some(json!({"theme": "dark"})));
}

#[test]
fn settings_are_kept_and_a_save_too_large_is_refused() {
    let root = tempfile::tempdir().expect("root");
    let store = FileSettingsStore::new(root.path().to_path_buf());
    assert_eq!(store.load(), None, "never saved");
    store.save(&json!({"theme": "dark"})).expect("save");
    assert_eq!(store.load(), Some(json!({"theme": "dark"})));
    let huge = json!({ "x": "a".repeat(AT_MOST) });
    assert_eq!(store.save(&huge), Err(SettingsError::TooLarge));
    assert_eq!(SettingsError::TooLarge.to_string(), "these settings are too large to keep");
    assert_eq!(store.load(), Some(json!({"theme": "dark"})), "a refused save keeps the last");
}

#[test]
fn a_settings_file_that_does_not_parse_is_kept_aside_not_overwritten() {
    let root = tempfile::tempdir().expect("root");
    let store = FileSettingsStore::new(root.path().to_path_buf());
    let folder = root.path().join(OWNER_DIR);
    std::fs::create_dir_all(&folder).expect("folder");
    std::fs::write(folder.join("settings.json"), "{\"model\": \"opus\", oops").expect("write");

    assert_eq!(store.load(), None);

    let aside = kept_aside(&folder);
    assert_eq!(aside.len(), 1, "{aside:?}");
    assert_eq!(std::fs::read_to_string(&aside[0]).expect("kept aside"), "{\"model\": \"opus\", oops");
}
