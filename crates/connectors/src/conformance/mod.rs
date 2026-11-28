mod cases;
mod fixture;
mod mock;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use serde_json::Map;

pub use fixture::FILE;
use fixture::{ACCOUNTS, Fixture};

use crate::provider::Provider;
use crate::scripted::{ENTRY, FOLDER, ScriptedProvider};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub findings: Vec<String>,
    pub ids: BTreeSet<String>,
}

pub fn connectors(content: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(content.join(FOLDER)) else { return Vec::new() };
    let mut found: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join(ENTRY).is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .collect();
    found.sort();
    found
}

pub fn run(content: &Path) -> Report {
    crate::memory::install(Arc::new(crate::memory::MemoryValues::default()));
    let names = connectors(content);
    let checked = platform::parallel::ordered(&names, |name| checked_alone(content, name));
    let mut report = Report::default();
    for (name, (findings, ids)) in names.iter().zip(checked) {
        let shown = format!("conformance {FOLDER}{name}");
        report.findings.extend(findings.into_iter().map(|why| format!("{shown}: {why}")));
        report.ids.extend(ids);
    }
    report
}

fn checked_alone(content: &Path, name: &str) -> (Vec<String>, BTreeSet<String>) {
    match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime.block_on(connector(content, name)),
        Err(error) => (vec![format!("the harness could not start: {error}")], BTreeSet::new()),
    }
}

fn fixture_of(content: &Path, name: &str) -> Result<Fixture, String> {
    let path = content.join(FOLDER).join(name).join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|_| format!("declares no {FILE}: every connector ships its fixtures"))?;
    fixture::parsed(&text)
}

struct Loader<'a> {
    content: &'a Path,
    entry: String,
    config: Map<String, serde_json::Value>,
}

impl Loader<'_> {
    fn load(&self) -> Result<ScriptedProvider, String> {
        let built = crate::config::Configuration::built();
        ScriptedProvider::configured(Some(self.content), &self.entry, &built, &self.config)
    }
}

pub async fn connector(content: &Path, name: &str) -> (Vec<String>, BTreeSet<String>) {
    let fixture = fixture_of(content, name);
    let config = fixture.as_ref().map(|fixture| fixture.config.clone()).unwrap_or_else(|_| Map::new());
    let loader = Loader { content, entry: format!("{FOLDER}{name}/{ENTRY}"), config };
    let provider = match loader.load() {
        Ok(provider) => provider,
        Err(error) => return (vec![format!("does not load: {error}")], BTreeSet::new()),
    };
    let mut found = declared(&provider);
    let ids = provider.descriptor().published();
    match fixture {
        Err(why) => found.push(why),
        Ok(fixture) => found.extend(exercised(&loader, &provider, &fixture).await),
    }
    (found, ids)
}

fn declared(provider: &dyn Provider) -> Vec<String> {
    provider
        .descriptor()
        .actions
        .iter()
        .filter(|action| action.profile.effects.is_empty())
        .map(|action| format!("{} declares no effects: say what it does (read, write, send-external…)", action.name))
        .collect()
}

async fn exercised(loader: &Loader<'_>, provider: &dyn Provider, fixture: &Fixture) -> Vec<String> {
    let descriptor = provider.descriptor();
    let mut found = Vec::new();
    let mut covered = BTreeSet::new();
    for case in &fixture.cases {
        if case.action != ACCOUNTS && descriptor.action(&case.action).is_none() {
            found.push(format!("{}: the connector offers no action {}", case.name, case.action));
            continue;
        }
        covered.insert(case.action.clone());
        let problems = match loader.load() {
            Ok(fresh) => cases::run(&fresh, fixture, case).await,
            Err(error) => vec![format!("does not load again: {error}")],
        };
        found.extend(problems.into_iter().map(|why| format!("{}: {why}", case.name)));
    }
    for action in fixture.handles.keys().filter(|action| *action != ACCOUNTS && descriptor.action(action).is_none()) {
        found.push(format!("handles names {action}, which the connector does not offer"));
    }
    let shown = descriptor.actions.iter().filter(|action| !action.hidden());
    for action in shown.filter(|action| !covered.contains(&action.name)) {
        found.push(format!("{} has no case in {FILE}", action.name));
    }
    found
}

#[cfg(test)]
mod tests;
