mod cron;

use std::sync::Arc;
use std::time::Duration;

use jiff::{ToSpan, Zoned};
use serde_json::{Value, json};
use store::Application;

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

fn chosen(application: &Application, field: &str) -> bool {
    let value = serde_json::to_value(application).unwrap_or(Value::Null);
    value[field]["connection"].as_str().is_some_and(|connection| !connection.is_empty())
}

fn targets(shared: &Shared, filter: &Value) -> Result<Vec<Option<Application>>> {
    let wanted = &filter["applications"];
    if wanted.is_null() || wanted == &json!(false) {
        return Ok(vec![None]);
    }
    let field = wanted.as_str();
    let applications = shared.store.applications()?;
    Ok(applications.into_iter().filter(|application| field.is_none_or(|field| chosen(application, field))).map(Some).collect())
}

fn subject_of(workflow: &str, application: Option<&Application>, slot: &Zoned) -> String {
    let owner = application.map_or("-", |application| application.id.as_str());
    format!("{KIND}:{workflow}:{owner}:{}", slot.strftime("%Y-%m-%dT%H:%M"))
}

fn input_of(shared: &Shared, workflow: &str, filter: &Value, application: Option<&Application>, slot: &Zoned) -> Value {
    let subject = subject_of(workflow, application, slot);
    let named = filter["title"].as_str().filter(|title| !title.trim().is_empty()).unwrap_or(workflow);
    let day = slot.strftime("%Y-%m-%d").to_string();
    let title = match application {
        Some(application) => format!("{named} · {} · {day}", application.name),
        None => format!("{named} · {day}"),
    };
    let mut input = json!({
        "title": title,
        "schedule": { "subject": subject, "cron": filter["cron"], "slot": slot.timestamp().to_string(), "day": day },
        "sources": [{ "kind": KIND, "subject": subject }],
    });
    if let Some(application) = application {
        input["application"] = crate::provenance::snapshot(&shared.store, application);
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
                for application in targets(&self.shared, filter)? {
                    if seen(&subject_of(&workflow, application.as_ref(), &slot)) {
                        continue;
                    }
                    let input = input_of(&self.shared, &workflow, filter, application.as_ref(), &slot);
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
