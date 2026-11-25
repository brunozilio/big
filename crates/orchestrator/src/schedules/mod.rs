mod cron;

use std::sync::Arc;
use std::time::Duration;

use jiff::{ToSpan, Zoned};
use serde_json::{Value, json};
use store::{Application, Repository};

pub use cron::Cron;

use crate::api::Orchestrator;
use crate::error::Result;
use crate::shared::Shared;

pub const KIND: &str = "schedule";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedules {
    pub enabled: bool,
    pub tick: Duration,
    pub window: Duration,
}

impl Schedules {
    pub fn from_config() -> Schedules {
        let seconds = |text: &str, fallback: u64| text.trim().parse().unwrap_or(fallback);
        Schedules {
            enabled: true,
            tick: Duration::from_secs(seconds(brand::workflow::SCHEDULE_TICK_SECONDS, 30).max(1)),
            window: Duration::from_secs(seconds(brand::workflow::SCHEDULE_CATCH_UP_MINUTES, 360) * 60),
        }
    }
}

fn minute_of(now: &Zoned) -> Option<Zoned> {
    now.with().second(0).subsec_nanosecond(0).build().ok()
}

pub fn latest(cron: &Cron, now: &Zoned, window: Duration) -> Option<Zoned> {
    let mut at = minute_of(now)?;
    for _ in 0..=window.as_secs() / 60 {
        if cron.matches(at.datetime()) {
            return Some(at);
        }
        at = at.checked_sub(1.minute()).ok()?;
    }
    None
}

fn in_zone(now: &Zoned, filter: &Value) -> Zoned {
    let zone = filter["zone"].as_str().filter(|zone| !zone.is_empty());
    zone.and_then(|zone| now.in_tz(zone).ok()).unwrap_or_else(|| now.clone())
}

struct Target {
    application: Application,
    repository: Option<String>,
    label: Option<String>,
}

fn label_of(repository: &Repository) -> String {
    let folder = repository.directory.rsplit(['/', '\\']).find(|part| !part.is_empty()).unwrap_or_default();
    repository.bound(connectors::contracts::CODE).map_or_else(|| folder.to_owned(), |code| code.value.clone())
}

fn whole(application: Application) -> Target {
    Target { application, repository: None, label: None }
}

fn expand(application: Application, field: Option<&str>) -> Vec<Target> {
    let Some(field) = field else { return vec![whole(application)] };
    if application.bound(field).is_some() {
        return vec![whole(application)];
    }
    let several = application.repositories.len() > 1;
    let chosen = application
        .repositories
        .iter()
        .filter(|repository| repository.bound(field).is_some());
    chosen
        .map(|repository| Target {
            application: Application { repositories: vec![repository.clone()], ..application.clone() },
            repository: Some(repository.id.clone()),
            label: several.then(|| label_of(repository)),
        })
        .collect()
}

fn targets(shared: &Shared, filter: &Value) -> Result<Vec<Option<Target>>> {
    let wanted = &filter["applications"];
    if wanted.is_null() || wanted == &json!(false) {
        return Ok(vec![None]);
    }
    let field = wanted.as_str();
    let applications = shared.store.applications()?;
    Ok(applications.into_iter().flat_map(|application| expand(application, field)).map(Some).collect())
}

fn subject_of(workflow: &str, target: Option<&Target>, slot: &Zoned) -> String {
    let owner = match target {
        Some(Target { application, repository: Some(repository), .. }) => format!("{}/{repository}", application.id),
        Some(target) => target.application.id.clone(),
        None => "-".to_owned(),
    };
    format!("{KIND}:{workflow}:{owner}:{}", slot.strftime("%Y-%m-%dT%H:%M"))
}

fn input_of(shared: &Shared, workflow: &str, filter: &Value, target: Option<&Target>, slot: &Zoned) -> Value {
    let subject = subject_of(workflow, target, slot);
    let named = filter["title"].as_str().filter(|title| !title.trim().is_empty()).unwrap_or(workflow);
    let day = slot.strftime("%Y-%m-%d").to_string();
    let owner = target.map(|target| match &target.label {
        Some(label) => format!("{} · {label}", target.application.name),
        None => target.application.name.clone(),
    });
    let title = match owner {
        Some(owner) => format!("{named} · {owner} · {day}"),
        None => format!("{named} · {day}"),
    };
    let mut source = json!({ "kind": KIND, "subject": subject });
    if let Some(repository) = target.and_then(|target| target.repository.as_ref()) {
        source["repository"] = json!(repository);
    }
    let mut input = json!({
        "title": title,
        "schedule": { "subject": subject, "cron": filter["cron"], "slot": slot.timestamp().to_string(), "day": day },
        "sources": [source],
    });
    if let Some(target) = target {
        input["application"] = crate::provenance::snapshot(&shared.store, &target.application);
    }
    input
}

impl Orchestrator {
    pub fn tick_schedules(&self, now: &Zoned) -> Result<usize> {
        let window = self.shared.config.schedules.window;
        let existing = self.shared.store.items()?;
        let seen = |subject: &str| existing.iter().any(|item| item.input["schedule"]["subject"] == subject);
        let mut created = 0;
        for workflow in self.workflows() {
            let Some(shape) = self.shape(&workflow) else { continue };
            for trigger in shape["triggers"].as_array().into_iter().flatten().filter(|on| on["kind"] == KIND) {
                let filter = &trigger["filter"];
                let text = filter["cron"].as_str().unwrap_or_default();
                let cron = match Cron::parse(text) {
                    Ok(cron) => cron,
                    Err(error) => {
                        tracing::warn!(%workflow, %error, "the workflow schedule is not a valid cron expression");
                        continue;
                    }
                };
                let Some(slot) = latest(&cron, &in_zone(now, filter), window) else { continue };
                for target in targets(&self.shared, filter)? {
                    if seen(&subject_of(&workflow, target.as_ref(), &slot)) {
                        continue;
                    }
                    let input = input_of(&self.shared, &workflow, filter, target.as_ref(), &slot);
                    self.create_item(&workflow, input)?;
                    created += 1;
                }
            }
        }
        Ok(created)
    }
}

pub fn watch(shared: &Arc<Shared>) {
    let schedules = shared.config.schedules;
    if !schedules.enabled {
        return;
    }
    let weak = Arc::downgrade(shared);
    shared.spawn(async move {
        loop {
            let Some(shared) = weak.upgrade() else { return };
            let orchestrator = Orchestrator::of(shared);
            let ticked = tokio::task::spawn_blocking(move || orchestrator.tick_schedules(&Zoned::now())).await;
            if let Ok(Err(error)) = ticked {
                tracing::warn!(%error, "the workflow schedules could not run");
            }
            tokio::time::sleep(schedules.tick).await;
        }
    });
}

#[cfg(test)]
mod tests;
