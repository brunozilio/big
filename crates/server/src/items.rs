use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use connectors::ProviderDescriptor;
use domain::workflow::{ItemId, StepState};
use orchestrator::ItemSummary;
use serde_json::{Value, json};
use store::ConnectionRecord;

use crate::blocks::{self, Place};
use crate::error::{ApiError, ApiResult, done, json as reply};
use crate::item_view::{self, Known};
use crate::marks::iso;
use crate::state::{Ctx, Shared};

const DEFAULT_WORKFLOW: &str = "default";
const MOST_TITLE: usize = 120;

struct Context {
    connections: Vec<ConnectionRecord>,
    descriptors: Vec<ProviderDescriptor>,
    labels: HashMap<String, Value>,
}

impl Context {
    fn load(shared: &Shared) -> ApiResult<Context> {
        Ok(Context {
            connections: shared.backend.store.connections()?,
            descriptors: shared.backend.connectors.descriptors().into_iter().cloned().collect(),
            labels: HashMap::new(),
        })
    }

    fn labels(&mut self, shared: &Shared, workflow: &str) -> Value {
        let found = self.labels.entry(workflow.to_owned());
        found.or_insert_with(|| shared.backend.orchestrator.describe(workflow)["labels"].clone()).clone()
    }

    fn item(&mut self, shared: &Shared, summary: &ItemSummary) -> ApiResult<Value> {
        let labels = self.labels(shared, &summary.workflow);
        let item = shared.backend.orchestrator.item(&summary.id)?;
        let known = Known { connections: &self.connections, descriptors: &self.descriptors, labels: &labels };
        let (value, _) = shared.stamps.stamp(item_view::item(&known, summary, item.as_ref()));
        Ok(value)
    }
}

pub fn all(shared: &Shared) -> ApiResult<Vec<Value>> {
    let mut context = Context::load(shared)?;
    let summaries = shared.backend.orchestrator.items()?;
    let top = summaries.iter().filter(|summary| summary.parent.is_none());
    let mut listed: Vec<Value> = top.map(|summary| context.item(shared, summary)).collect::<ApiResult<_>>()?;
    listed.sort_by(|left, right| right["createdAt"].as_str().cmp(&left["createdAt"].as_str()));
    Ok(listed)
}

pub fn changed(shared: &Shared, id: &ItemId) -> ApiResult<Option<Value>> {
    let Some(summary) = shared.backend.orchestrator.summary(id)? else { return Ok(None) };
    if summary.parent.is_some() {
        return Ok(None);
    }
    let mut context = Context::load(shared)?;
    let labels = context.labels(shared, &summary.workflow);
    let item = shared.backend.orchestrator.item(id)?;
    let known = Known { connections: &context.connections, descriptors: &context.descriptors, labels: &labels };
    let (value, moved) = shared.stamps.stamp(item_view::item(&known, &summary, item.as_ref()));
    Ok(moved.then_some(value))
}

pub async fn list(State(ctx): State<Ctx>) -> ApiResult<Response> {
    Ok(reply(all(&ctx)?))
}

fn workflow(shared: &Shared) -> ApiResult<String> {
    let names = shared.backend.orchestrator.workflows();
    let chosen = names.iter().find(|name| *name == DEFAULT_WORKFLOW).or(names.first());
    chosen.cloned().ok_or_else(|| ApiError::failed("Nenhum workflow encontrado."))
}

fn input(shared: &Shared, body: &Value) -> ApiResult<Value> {
    let text = body["text"].as_str().map(str::trim).unwrap_or_default();
    let issue = body["issue"]["key"].as_str().filter(|key| !key.is_empty());
    if text.is_empty() && issue.is_none() {
        return Err(ApiError::bad("Descreva o item ou escolha uma issue."));
    }
    let first: String = text.lines().next().unwrap_or_default().chars().take(MOST_TITLE).collect();
    let title = if first.is_empty() { issue.unwrap_or_default().to_owned() } else { first };
    let files = crate::attachments::paths(&shared.backend.attachments(), &body["attachments"]);
    let files: Vec<String> = files.iter().map(|file| file.display().to_string()).collect();
    let mut input = json!({ "title": title, "text": text, "attachments": files });
    if !text.is_empty() {
        input["description"] = json!(text);
    }
    if let Some(key) = issue {
        input["issue"] = json!(key);
        input["connection"] = body["issue"]["connection"].clone();
    } else if let Some(project) = body["project"]["key"].as_str().filter(|key| !key.is_empty()) {
        input["project"] = json!(project);
        input["connection"] = body["project"]["connection"].clone();
    }
    if let Some(model) = body["model"].as_str().filter(|model| !model.is_empty()) {
        input["model"] = json!(model);
    }
    Ok(input)
}

pub async fn create(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    let input = input(&ctx, &body)?;
    let id = ctx.backend.orchestrator.create_item(&workflow(&ctx)?, input)?;
    let summary = ctx.backend.orchestrator.summary(&id)?.ok_or_else(|| ApiError::failed("O item sumiu."))?;
    let item = Context::load(&ctx)?.item(&ctx, &summary)?;
    ctx.live.item(item.clone());
    Ok(reply(item))
}

fn item_id(id: &str) -> ItemId {
    ItemId::from(id)
}

fn known(shared: &Shared, id: &ItemId) -> ApiResult<()> {
    match shared.backend.orchestrator.item(id)? {
        Some(_) => Ok(()),
        None => Err(ApiError::missing("Não há esse item.")),
    }
}

pub async fn rerun(State(ctx): State<Ctx>, Path(id): Path<String>, body: Option<Json<Value>>) -> ApiResult<Response> {
    let id = item_id(&id);
    known(&ctx, &id)?;
    let step = body.and_then(|Json(body)| body["step"].as_str().map(str::to_owned));
    match step {
        Some(step) => ctx.backend.orchestrator.rewind(&id, &step).await?,
        None => ctx.backend.orchestrator.rerun(&id).await?,
    }
    Ok(done())
}

pub async fn cancel(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let id = item_id(&id);
    known(&ctx, &id)?;
    ctx.backend.orchestrator.cancel(&id)?;
    Ok(done())
}

pub async fn step(
    State(ctx): State<Ctx>,
    Path((id, step)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let id = item_id(&id);
    let item = ctx.backend.orchestrator.item(&id)?.ok_or_else(|| ApiError::missing("Não há esse item."))?;
    let attempt = query.get("attempt").and_then(|text| text.parse::<u32>().ok());
    let view = ctx.backend.orchestrator.step_view(&id, &step, attempt).await?;
    let latest = view.attempts.last().map_or(0, |last| last.n);
    let run = item.steps.iter().rev().find(|run| run.id == step);
    let waiting = view.attempt == latest && run.is_some_and(|run| run.state == StepState::WaitingHuman);
    let connections = ctx.backend.store.connections()?;
    let place = Place { connections: &connections, waiting };
    let attempts: Vec<Value> = view
        .attempts
        .iter()
        .map(|attempt| {
            json!({
                "n": attempt.n, "state": item_view::state(attempt.state),
                "startedAt": iso(attempt.started_at), "endedAt": attempt.ended_at.map(iso),
            })
        })
        .collect();
    let mut blocks = blocks::normalized(&place, &view.blocks);
    let stuck = item.steps.iter().any(|run| run.id.is_empty() && run.state == StepState::WaitingHuman);
    if stuck && run.is_some_and(|run| run.state == StepState::Failed) {
        let decided = ctx.backend.orchestrator.step_view(&id, "", None).await?;
        let place = Place { connections: &connections, waiting: true };
        blocks.extend(blocks::normalized(&place, &decided.blocks));
    }
    Ok(reply(
        json!({ "item": id.as_str(), "step": step, "attempt": view.attempt, "attempts": attempts, "blocks": blocks }),
    ))
}

pub async fn human(
    State(ctx): State<Ctx>,
    Path((id, step)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let id = item_id(&id);
    let action = body["action"].as_str().filter(|action| !action.is_empty());
    let action = action.ok_or_else(|| ApiError::bad("Diga a ação."))?;
    let (action, payload) = blocks::input(action, body["payload"].clone());
    ctx.backend.orchestrator.human_input(&id, &step, &action, payload)?;
    Ok(done())
}
