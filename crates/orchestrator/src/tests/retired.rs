use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata};

use crate::activation::Script;
use crate::resources::Resources;

const OLD: &str = r#"
import { workflow } from "sdk";
import { develop, review } from "sdk/steps";

export default workflow({
  name: "velho",
  steps: [develop({ agent: "developer" }), review({ onChanges: "develop" })],
});
"#;

#[derive(Clone, Default)]
struct Heard(Arc<Mutex<Vec<String>>>);

struct Line(String);

impl Visit for Line {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.push_str(&format!("{}={value:?} ", field.name()));
    }
}

impl tracing::Subscriber for Heard {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut line = Line(String::new());
        event.record(&mut line);
        self.0.lock().expect("lock").push(line.0);
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

#[test]
fn an_old_workflow_that_imports_review_from_the_sdk_keeps_the_pull_request_review_and_is_warned() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join("workflows/velho.ts");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
    std::fs::write(&file, OLD).expect("write");
    let resources = Resources::new(Some(dir.path().to_path_buf()), None, Arc::new(content::builtin_text));
    let heard = Heard::default();
    let loaded = tracing::subscriber::with_default(heard.clone(), || Script::load(&resources, "velho"));
    let script = loaded.expect("the old workflow still loads");
    let steps = script.described["workflows"][0]["steps"].as_array().cloned().unwrap_or_default();
    let kinds: Vec<&str> = steps.iter().filter_map(|step| step["step"].as_str()).collect();
    assert_eq!(kinds, ["develop", "review"], "review is still the pull request review");
    assert_eq!(script.described["labels"]["review"]["requires"], "code");
    let lines = heard.0.lock().expect("lock").clone();
    let warned = lines.iter().find(|line| line.contains("leave this module soon")).cloned().unwrap_or_default();
    assert!(warned.contains("develop, review") && warned.contains("workflows/velho.ts"), "{lines:?}");
    assert!(
        lines.iter().any(|line| line.contains("sdk/steps/general.js") && line.contains("name=review")),
        "{lines:?}"
    );
}
