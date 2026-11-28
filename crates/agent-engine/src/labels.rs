use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, PoisonError};

use domain::label::{Levels, Mark, Mode, Read, Sink, Trust};
use domain::permission::{Reason, Verdict};
use serde_json::{Value, json};

use crate::policy_gate::Asked;

pub const FILE: &str = "safety/labels.ts";
const EXPORT: &str = "labels";
pub const CLASSIFIERS: &str = "safety/classifiers/";
const CLASSIFY: &str = "classify";

static LEVELS: Mutex<Option<(String, Levels)>> = Mutex::new(None);

struct Bare;

impl script::Host for Bare {
    fn call(&self, function: &str, _arguments: Value) -> Result<Value, String> {
        Err(format!("a classifier reaches nothing on the host, yet it called {function}"))
    }
}

pub fn levels() -> Levels {
    let folder = content::folder();
    let source = content::user_text(folder.as_deref(), FILE).or_else(|| content::builtin_text(FILE));
    source.map(|source| levels_of(&source)).unwrap_or_default()
}

pub fn levels_of(source: &str) -> Levels {
    let mut kept = LEVELS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((_, levels)) = kept.as_ref().filter(|(known, _)| known == source) {
        return levels.clone();
    }
    let read = script::exported(FILE, source, EXPORT).and_then(|value| Levels::of(&value));
    let levels = read
        .inspect_err(|error| tracing::warn!(%error, file = FILE, "the sensitivity levels did not load, so they stay in shadow"))
        .unwrap_or_default();
    *kept = Some((source.to_owned(), levels.clone()));
    levels
}

pub fn mode(levels: &Levels) -> Mode {
    if levels.enforced() { Mode::Enforce } else { Mode::Shadow }
}

pub fn flow(asked: Asked<'_>, tool: &str, levels: &Levels, mark: &Mark, sink: &Sink<'_>) -> Verdict {
    let Some(why) = mark.crossing(levels, sink) else { return Verdict::Allow };
    tracing::warn!(
        target: "audit",
        surface = asked.surface.word(),
        stage = asked.stage,
        user = asked.owner,
        tool,
        reason = why.as_str(),
        label = mark.label.as_deref().unwrap_or_default(),
        ceiling = sink.ceiling.unwrap_or_default(),
        mode = mode(levels).word(),
        "label.crossed"
    );
    if levels.enforced() { Verdict::Ask { reason: Reason::Flow(why), rememberable: false } } else { Verdict::Allow }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classifier {
    pub path: String,
    pub source: String,
}

pub fn classifiers() -> Vec<Classifier> {
    let folder = content::folder();
    let scripts = content::listed(CLASSIFIERS).into_iter().filter(|path| path.ends_with(".ts") || path.ends_with(".js"));
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

#[cfg(test)]
mod tests;
