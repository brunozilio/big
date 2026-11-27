use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_engine::EventSink;
use domain::effect::{Effect, EffectOutcome};
use domain::workflow::{HeadsUpNote, ItemId, ItemRecap, WorkItem, WorkflowEvent};
use domain::{ChoreAnswer, EngineEvent, ModelOption, PromptRequest, RunId, SessionCommand, SessionId};
use tokio::sync::mpsc;

use crate::shared::Shared;

const WAIT: Duration = Duration::from_secs(60);
const SHORT_ID: usize = 6;

pub struct Chore {
    pub command: SessionCommand,
    pub text: String,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub resume: Option<String>,
}

pub fn model(shared: &Shared, wanted: Option<&str>) -> ModelOption {
    let models = shared.config.engine.models();
    let exact = wanted.and_then(|wanted| models.iter().find(|model| model.id == wanted));
    let near = || wanted.and_then(|wanted| models.iter().find(|model| model.id.starts_with(wanted)));
    exact
        .or_else(near)
        .or(models.first())
        .cloned()
        .unwrap_or_else(|| ModelOption::unlisted(wanted.unwrap_or("unknown")))
}

pub async fn answer(shared: &Shared, chore: Chore) -> Option<ChoreAnswer> {
    let mut request = PromptRequest::plain(chore.cwd, chore.text, model(shared, chore.model.as_deref()));
    request.session.command = Some(chore.command);
    request.session.resume = chore.resume.map(SessionId);
    request.session.language = shared.store.language().ok();
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let sink: Arc<dyn EventSink> = Arc::new(move |_run: RunId, event: EngineEvent| {
        let _ = sender.send(event);
    });
    let _handle = shared.config.engine.start(RunId::new(), &request, sink).ok()?;
    let waited = tokio::time::timeout(WAIT, async {
        while let Some(event) = receiver.recv().await {
            match event {
                EngineEvent::Chore(answer) => return Some(answer),
                EngineEvent::Completed { .. }
                | EngineEvent::Failed(_)
                | EngineEvent::Failure(_)
                | EngineEvent::Cancelled => return None,
                _ => {}
            }
        }
        None
    });
    waited.await.ok().flatten()
}

pub struct Ran {
    pub key: String,
    pub session: String,
    pub cwd: PathBuf,
    pub step: String,
    pub agent: String,
}

pub fn runs(item: &WorkItem) -> Vec<Ran> {
    let mut found: Vec<((u64, u32), Ran)> = item
        .effects
        .values()
        .filter(|entry| matches!(entry.request.effect, Effect::AgentRun { .. }))
        .filter_map(|entry| {
            let done = match &entry.outcome {
                Some(EffectOutcome::Ok { value }) => Some(value),
                _ => None,
            };
            let field = |name: &str| {
                done.and_then(|value| value[name].as_str()).or_else(|| entry.progress[name].as_str()).map(str::to_owned)
            };
            let Effect::AgentRun { agent, .. } = &entry.request.effect else { return None };
            let ran = Ran {
                key: entry.request.key.to_string(),
                session: field("session")?,
                cwd: field("worktree")?.into(),
                step: entry.request.key.step.clone(),
                agent: agent.clone(),
            };
            Some(((entry.request.key.seq, entry.request.key.attempt), ran))
        })
        .collect();
    found.sort_by_key(|(order, _)| *order);
    found.into_iter().map(|(_, ran)| ran).collect()
}

pub fn latest_run(item: &WorkItem) -> Option<Ran> {
    runs(item).pop()
}

pub(crate) fn wanted_model(item: &WorkItem) -> Option<String> {
    item.input["model"].as_str().filter(|model| !model.is_empty()).map(str::to_owned)
}

pub fn recap(shared: &Arc<Shared>, item: &ItemId, moment: &str) {
    let Ok(state) = shared.item(item) else { return };
    let Some(ran) = latest_run(&state) else { return };
    let after = format!("{}#{moment}", ran.key);
    if state.recap.as_ref().is_some_and(|recap| recap.after == after) {
        return;
    }
    let (shared, item) = (Arc::clone(shared), item.clone());
    let held = Arc::clone(&shared);
    held.spawn(async move {
        let chore = Chore {
            command: SessionCommand::Recap,
            text: String::new(),
            cwd: ran.cwd,
            model: wanted_model(&state),
            resume: Some(ran.session),
        };
        let Some(ChoreAnswer::Recap(text)) = answer(&shared, chore).await else { return };
        let recap = ItemRecap { text, after };
        if let Err(error) = shared.append(&item, &[WorkflowEvent::Recapped { recap }]) {
            tracing::warn!(%error, "the item's recap could not be kept");
        }
    });
}

pub fn heads_up(shared: &Arc<Shared>, item: &ItemId, step: &str, session: String, cwd: PathBuf) {
    let Ok(state) = shared.item(item) else { return };
    let (shared, item, step) = (Arc::clone(shared), item.clone(), step.to_owned());
    let held = Arc::clone(&shared);
    held.spawn(async move {
        let chore = Chore {
            command: SessionCommand::HeadsUp,
            text: String::new(),
            cwd,
            model: wanted_model(&state),
            resume: Some(session),
        };
        let Some(ChoreAnswer::HeadsUp(found)) = answer(&shared, chore).await else { return };
        let note = HeadsUpNote { title: found.title, explain: found.explain, urgent: found.urgent };
        if let Err(error) = shared.append(&item, &[WorkflowEvent::HeadsUp { step, note }]) {
            tracing::warn!(%error, "a heads-up could not be kept");
        }
    });
}

fn source_text(item: &WorkItem) -> Option<String> {
    let input = &item.input;
    let issue = input["issue"]["key"].as_str().or(input["issue"].as_str());
    item.name.clone().or_else(|| {
        input["text"]
            .as_str()
            .or(input["title"].as_str())
            .or(issue)
            .map(str::to_owned)
            .filter(|text| !text.trim().is_empty())
    })
}

pub fn branch_for(item: &WorkItem) -> Option<String> {
    let slug = domain::workflow::branch_slug(&source_text(item)?)?;
    let id: String = item.id.as_str().chars().filter(char::is_ascii_alphanumeric).collect();
    let short = &id[id.len().saturating_sub(SHORT_ID)..];
    Some(format!("{}/{slug}-{}", brand::workflow::BRANCH_PREFIX, short.to_ascii_lowercase()))
}

pub fn ensure_branch(shared: &Shared, item: &WorkItem, current: &str) -> WorkItem {
    let started = item
        .effects
        .iter()
        .any(|(key, entry)| key != current && matches!(entry.request.effect, Effect::AgentRun { .. }));
    if item.branch.is_some() || started {
        return item.clone();
    }
    let Some(name) = branch_for(item) else { return item.clone() };
    shared.append(&item.id, &[WorkflowEvent::Branched { name }]).unwrap_or_else(|error| {
        tracing::warn!(%error, "the item's branch name could not be kept");
        item.clone()
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn item(input: serde_json::Value, name: Option<&str>) -> WorkItem {
        WorkItem {
            id: ItemId::from("0f3a9c2e-77b1-4d2a-9e10-ab12cd34ef56"),
            input,
            name: name.map(str::to_owned),
            ..WorkItem::default()
        }
    }

    #[test]
    fn an_item_s_branch_reads_like_its_title_and_ends_with_its_id() {
        let named = item(json!({ "text": "x" }), Some("Corrigir a validação do login"));
        let expected = format!("{}/corrigir-a-validacao-do-login-34ef56", brand::workflow::BRANCH_PREFIX);
        assert_eq!(branch_for(&named).as_deref(), Some(expected.as_str()));
        let texted = item(json!({ "text": "Adicionar exportação CSV" }), None);
        assert!(branch_for(&texted).is_some_and(|branch| branch.ends_with("/adicionar-exportacao-csv-34ef56")));
        let issued = item(json!({ "issue": { "key": "PROJ-12" } }), None);
        assert!(branch_for(&issued).is_some_and(|branch| branch.ends_with("/proj-12-34ef56")));
        assert_eq!(branch_for(&item(json!({}), None)), None);
    }
}
