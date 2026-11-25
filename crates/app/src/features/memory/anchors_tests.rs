use std::sync::Arc;

use domain::{Author, MemoryBrief, MemoryEdit, MemoryKind, ModelOption, PromptRequest, Provenance, RunId};

use super::Memories;
use super::store::InMemoryStore;

fn remember(text: &str, touches: &[&str]) -> MemoryEdit {
    MemoryEdit::Remember {
        kind: MemoryKind::Semantic,
        text: text.into(),
        tags: Vec::new(),
        touches: touches.iter().map(|touch| (*touch).to_owned()).collect(),
        scope: Default::default(),
    }
}

#[test]
fn a_run_is_handed_the_memories_about_files_that_its_prompt_did_not_recall() {
    let project = tempfile::tempdir().expect("project");
    let memories = Memories::new(Arc::new(InMemoryStore::default()));
    let edits = [
        remember("Keep defaultRetryMax at 4", &["client.go"]),
        remember("The renderer draws signed distance fields", &[]),
    ];
    let by = Provenance { author: Author::User, now: 1, origin: None };
    memories.edit(project.path(), &edits, by).expect("edit");
    let request = PromptRequest::plain(project.path().into(), "Tune the renderer", ModelOption::new("m", "M"))
        .remembering(MemoryBrief::new(None));

    let briefed = memories.brief(RunId::new(), &request, false);

    let brief = briefed.host.memory.expect("brief");
    assert!(brief.anchors && brief.structured);
    let anchored: Vec<&str> = brief.anchored.iter().map(|memory| memory.text.as_str()).collect();
    let recalled: Vec<&str> = brief.recalled.iter().map(|memory| memory.text.as_str()).collect();
    assert_eq!(anchored.len() + recalled.len(), 2, "{anchored:?} {recalled:?}");
    assert!(!anchored.iter().any(|text| recalled.contains(text)), "a recalled memory is not handed twice");
    assert!(anchored.iter().chain(&recalled).any(|text| *text == "Keep defaultRetryMax at 4"));
    let told = brief.edits_format().expect("format");
    assert!(told.contains("\"touches\"") && told.contains("\"ruled_out\""), "{told}");
}

#[test]
fn the_context_features_follow_the_config_whatever_the_project_s_settings_say() {
    use brand::features as flags;
    use domain::features::on;
    let project = tempfile::tempdir().expect("project");
    let settings = project.path().join(brand::DIR);
    std::fs::create_dir_all(&settings).expect("settings folder");
    let off = serde_json::json!({ "features": {
        "experimental-memory-anchors": false,
        "experimental-structured-learnings": false,
        "experimental-learning-check": false,
        "experimental-memory-seed": false,
    } });
    std::fs::write(settings.join("settings.json"), off.to_string()).expect("settings");
    let memories = Memories::new(Arc::new(InMemoryStore::default()));
    let by = Provenance { author: Author::User, now: 1, origin: None };
    memories.edit(project.path(), &[remember("Keep retries at 4", &["client.go"])], by).expect("edit");
    let request = PromptRequest::plain(project.path().into(), "Tune the renderer", ModelOption::new("m", "M"))
        .remembering(MemoryBrief::new(None));

    let brief = memories.brief(RunId::new(), &request, false).host.memory.expect("brief");

    assert_eq!((brief.anchors, brief.structured), (on(flags::MEMORY_ANCHORS), on(flags::STRUCTURED_LEARNINGS)));
    let told = brief.edits_format().expect("format");
    assert_eq!(told.contains("\"touches\""), on(flags::MEMORY_ANCHORS), "{told}");
}
