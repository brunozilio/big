use std::path::Path;
use std::sync::Arc;

use domain::factory::{Finding, FindingKind, Origin as Source, crossed, findings, generation, recurring};
use domain::workflow::{ItemId, WorkItem, WorkflowEvent};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::api::Orchestrator;
use crate::error::Result;
use crate::resources::{FACTORIES, Origin, Resources};
use crate::shared::Shared;

pub const ESCAPED: &str = "factory.gate.escaped";
pub const RECURRING: &str = "factory.step.recurring";

const WATCHED: [&str; 8] = ["workflows", "steps", "views", "agents", "skills", "prompts", "sdk", "evals"];

const RECENT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Autonomy {
    In,
    Near,
    Out,
}

impl Autonomy {
    fn parse(text: &str) -> Autonomy {
        match text.trim() {
            "in" => Autonomy::In,
            "out" => Autonomy::Out,
            _ => Autonomy::Near,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Factory {
    pub enabled: bool,
    pub gates: Vec<String>,
    pub failure_threshold: u64,
    pub autonomy: Autonomy,
}

impl Factory {
    pub fn from_config() -> Factory {
        Factory {
            enabled: brand::factory::ENABLED.trim() == "true",
            gates: brand::factory::GATES
                .split(',')
                .map(str::trim)
                .filter(|gate| !gate.is_empty())
                .map(str::to_owned)
                .collect(),
            failure_threshold: brand::factory::FAILURE_THRESHOLD.trim().parse().unwrap_or(3),
            autonomy: Autonomy::parse(brand::factory::AUTONOMY),
        }
    }

    pub fn off() -> Factory {
        Factory { enabled: false, ..Factory::from_config() }
    }
}

pub fn relevant(events: &[WorkflowEvent]) -> bool {
    events.iter().any(|event| match event {
        WorkflowEvent::Signal { kind, .. } => kind == domain::factory::STEP_FINISHED,
        WorkflowEvent::Rewound { .. } => true,
        _ => false,
    })
}

pub fn is_factory_item(input: &Value) -> bool {
    input["factory"].is_object()
}

pub fn stamp(shared: &Shared, resources: &Resources, input: &mut Value) -> Result<()> {
    if !input["generation"].is_null() {
        return Ok(());
    }
    let (version, files) = fingerprint(resources);
    shared.store.record_generation(&version, resources.chain(), &files)?;
    if let Some(fields) = input.as_object_mut() {
        fields.insert("generation".into(), json!({ "version": version, "chain": resources.chain() }));
    }
    Ok(())
}

pub fn fingerprint(resources: &Resources) -> (String, Value) {
    let mut files: Vec<(String, String)> = vec![("builtin".into(), env!("CARGO_PKG_VERSION").into())];
    let mut factories = resources.chain().iter();
    for (origin, dir) in resources.layer_dirs() {
        let label = match origin {
            Origin::Repo => "repo".to_owned(),
            Origin::Factory => factories.next().cloned().unwrap_or_default(),
            Origin::User | Origin::Builtin => "user".to_owned(),
        };
        for folder in WATCHED {
            collect(&dir.join(folder), &format!("{label}/{folder}"), &mut files);
        }
    }
    let version = generation(files.iter().map(|(path, text)| (path.as_str(), text.as_str())));
    let manifest: Map<String, Value> =
        files.iter().map(|(path, text)| (path.clone(), json!(generation([(path.as_str(), text.as_str())])))).collect();
    (version, Value::Object(manifest))
}

fn collect(dir: &Path, label: &str, files: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == FACTORIES {
            continue;
        }
        let path = entry.path();
        let place = format!("{label}/{name}");
        if path.is_dir() {
            collect(&path, &place, files);
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            files.push((place, text));
        }
    }
}

pub fn watch(shared: &Arc<Shared>) {
    if !shared.config.factory.enabled {
        return;
    }
    let shared = Arc::clone(shared);
    let owner = Arc::clone(&shared);
    owner.spawn(async move {
        loop {
            let notified = shared.observing.notified();
            let pending: Vec<ItemId> =
                std::mem::take(&mut *shared.observed.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
            if pending.is_empty() {
                notified.await;
                continue;
            }
            let worker = Arc::clone(&shared);
            let done = tokio::task::spawn_blocking(move || {
                for id in pending {
                    if let Err(error) = observe(&worker, &id) {
                        tracing::warn!(item = %id, %error, "the factory could not read the item");
                    }
                }
            });
            let _ = done.await;
        }
    });
}

pub fn observe(shared: &Arc<Shared>, id: &ItemId) -> Result<()> {
    let factory = &shared.config.factory;
    let Some(item) = shared.store.item(id)? else { return Ok(()) };
    if !factory.enabled || is_factory_item(&item.input) {
        return Ok(());
    }
    let events: Vec<(i64, WorkflowEvent)> =
        shared.store.timed_events(id, 0)?.into_iter().map(|(_, at, event)| (at, event)).collect();
    let generation = item.input["generation"]["version"].as_str();
    let source = Source { item: id, workflow: &item.workflow, generation };
    let gates: Vec<&str> = factory.gates.iter().map(String::as_str).collect();
    let fresh = shared.store.record_findings(&findings(&source, &gates, &events))?;
    let orchestrator = Orchestrator::of(Arc::clone(shared));
    for finding in fresh {
        match finding.kind {
            FindingKind::Escape => {
                orchestrator.launch(ESCAPED, &item, &finding.id, &escape_data(&finding))?;
            }
            FindingKind::Failure => {
                let (count, same) = recurring(shared.store.findings(Some(FindingKind::Failure))?, &finding);
                if crossed(count, factory.failure_threshold) {
                    let data = recurring_data(&finding, count, same);
                    let generation = finding.generation.as_deref().unwrap_or("-");
                    let subject = format!("{}/{}/{generation}/{count}", finding.workflow, finding.step);
                    orchestrator.launch(RECURRING, &item, &subject, &data)?;
                }
            }
        }
    }
    Ok(())
}

fn escape_data(finding: &Finding) -> Value {
    json!({
        "workflow": finding.workflow,
        "step": finding.step,
        "gate": finding.gate,
        "caught_by": finding.caught_by,
        "generation": finding.generation,
        "findings": [finding],
    })
}

fn recurring_data(finding: &Finding, count: u64, mut same: Vec<Finding>) -> Value {
    let keep = same.len().saturating_sub(RECENT);
    same.drain(..keep);
    json!({
        "workflow": finding.workflow,
        "step": finding.step,
        "count": count,
        "generation": finding.generation,
        "findings": same,
    })
}

fn tuning(item: &WorkItem, data: &Value) -> bool {
    let factory = &item.input["factory"];
    item.ending.is_none()
        && factory["kind"] == RECURRING
        && factory["data"]["workflow"] == data["workflow"]
        && factory["data"]["step"] == data["step"]
}

impl Orchestrator {
    fn launch(&self, kind: &str, origin: &WorkItem, subject: &str, data: &Value) -> Result<usize> {
        let factory = &self.shared.config.factory;
        let resources = self.shared.resources_in(self.shared.home(&origin.input).as_deref());
        let event = json!({ "kind": kind, "subject": subject, "data": data });
        let existing = self.shared.store.items()?;
        if kind == RECURRING && existing.iter().any(|item| tuning(item, data)) {
            return Ok(0);
        }
        let mut created = 0;
        for workflow in self.triggered(kind, &event) {
            let known =
                existing.iter().any(|item| item.input["factory"]["subject"] == subject && item.workflow == workflow);
            if known {
                continue;
            }
            let base = resources.chain().first().and_then(|name| resources.factory_dir(name));
            let mut input = json!({
                "title": title(kind, data),
                "factory": {
                    "kind": kind,
                    "subject": subject,
                    "autonomy": factory.autonomy,
                    "origin": origin.id,
                    "base": base,
                    "dir": brand::DIR,
                    "chain": resources.chain(),
                    "data": data,
                },
                "event": event,
            });
            for field in ["application", "model", "effort"] {
                if !origin.input[field].is_null() {
                    input[field] = origin.input[field].clone();
                }
            }
            self.create_with(&workflow, input, None)?;
            created += 1;
        }
        Ok(created)
    }
}

fn title(kind: &str, data: &Value) -> String {
    let step = data["step"].as_str().unwrap_or_default();
    let workflow = data["workflow"].as_str().unwrap_or_default();
    if kind == ESCAPED {
        let gate = data["gate"].as_str().unwrap_or_default();
        return format!("Avaliação: {gate} deixou passar um defeito de {step} ({workflow})");
    }
    let count = data["count"].as_u64().unwrap_or_default();
    format!("Fábrica: {step} falhou {count} vezes ({workflow})")
}
