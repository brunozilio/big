use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use domain::label::{Levels, Mode, Read, Trust};
use serde_json::{Value, json};

pub const CLASSIFIERS: &str = "safety/classifiers/";
const CLASSIFY: &str = "classify";

struct Bare;

impl script::Host for Bare {
    fn call(&self, function: &str, _arguments: Value) -> Result<Value, String> {
        Err(format!("a classifier reaches nothing on the host, yet it called {function}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classifier {
    pub path: String,
    pub source: String,
}

pub fn classifiers() -> Vec<Classifier> {
    let folder = content::folder();
    let scripts =
        content::listed(CLASSIFIERS).into_iter().filter(|path| path.ends_with(".ts") || path.ends_with(".js"));
    scripts.map(|path| Classifier { source: content::text_in(folder.as_deref(), &path), path }).collect()
}

pub fn classified(levels: &Levels, read: Read, value: &Value) -> Read {
    classified_by(&classifiers(), levels, read, value)
}

pub fn classified_by(classifiers: &[Classifier], levels: &Levels, mut read: Read, value: &Value) -> Read {
    for classifier in classifiers {
        let Some((label, trust)) = verdict_of(classifier, &read, value) else { continue };
        let enforced = script::exported(&classifier.path, &classifier.source, "mode")
            .is_ok_and(|mode| mode.as_str() == Some(Mode::Enforce.word()));
        let raised = label.filter(|found| levels.ranked(Some(found)) > levels.ranked(read.label.as_deref()));
        let distrusted = trust == Some(Trust::External) && read.trust != Trust::External;
        if raised.is_none() && !distrusted {
            continue;
        }
        tracing::info!(
            target: "audit",
            classifier = classifier.path,
            source = read.source,
            label = raised.as_deref().unwrap_or_default(),
            external = distrusted,
            mode = if enforced { Mode::Enforce.word() } else { Mode::Shadow.word() },
            "label.classified"
        );
        if enforced {
            read.label = raised.or(read.label);
            read.trust = if distrusted { Trust::External } else { read.trust };
        }
    }
    read
}

fn verdict_of(classifier: &Classifier, read: &Read, value: &Value) -> Option<(Option<String>, Option<Trust>)> {
    let input = json!({ "source": read.source, "trust": read.trust, "label": read.label, "result": value });
    let host: Arc<dyn script::Host> = Arc::new(Bare);
    let stop = Arc::new(AtomicBool::new(false));
    let found = script::run(&classifier.path, &classifier.source, CLASSIFY, &[input], &Value::Null, host, stop)
        .inspect_err(|error| tracing::warn!(%error, classifier = classifier.path, "a classifier did not run"))
        .ok()?;
    let label = found["label"].as_str().map(str::trim).filter(|label| !label.is_empty()).map(str::to_owned);
    let trust = serde_json::from_value::<Trust>(found["trust"].clone()).ok();
    Some((label, trust))
}
