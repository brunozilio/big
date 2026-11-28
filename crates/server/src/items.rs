use std::collections::HashMap;

use crate::calls::connection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::{Extension, Json};
use connectors::ProviderDescriptor;
use connectors::contracts::{self, code};
use domain::principal::Principal;
use domain::workflow::{ItemId, StepState};
use orchestrator::ItemSummary;
use serde_json::{Map, Value, json};
use store::{Application, ConnectionRecord, Repository};

use crate::blocks::{self, Place};
use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::item_view::{self, Known};
use crate::marks::{iso, serving};
use crate::state::{Ctx, Shared};

const DEFAULT_WORKFLOW: &str = "default";
const MOST_TITLE: usize = 120;
const ISSUE: &str = "issue";
const PULL_REQUEST: &str = "pull_request";
const BRANCH: &str = "branch";
const KINDS: [&str; 3] = [PULL_REQUEST, ISSUE, BRANCH];

struct Context {
    connections: Vec<ConnectionRecord>,
    stale: Vec<String>,
    descriptors: Vec<ProviderDescriptor>,
    described: HashMap<String, Value>,
    archived: HashMap<String, i64>,
}

fn with_archive(mut value: Value, archived: &HashMap<String, i64>, id: &ItemId) -> Value {
    value["archivedAt"] = archived.get(id.as_str()).map_or(Value::Null, |at| json!(iso(*at)));
    value
}

impl Context {
    fn load(shared: &Shared) -> ApiResult<Context> {
        let connections = shared.backend.store.connections()?;
        Ok(Context {
            stale: stale(shared, &connections),
            connections,
            descriptors: shared.backend.connectors.descriptors().into_iter().cloned().collect(),
            described: HashMap::new(),
            archived: shared.backend.store.archived_items()?,
        })
    }

    fn described(&mut self, shared: &Shared, workflow: &str) -> Value {
        let found = self.described.entry(workflow.to_owned());
        found.or_insert_with(|| shared.backend.orchestrator.describe(workflow)).clone()
    }

    fn item(&mut self, shared: &Shared, summary: &ItemSummary) -> ApiResult<Value> {
        let described = self.described(shared, &summary.workflow);
        let item = shared.backend.orchestrator.item(&summary.id)?;
        let output = item.as_ref().and_then(|item| shared.backend.orchestrator.output_of(item));
        let known = Known {
            connections: &self.connections,
            stale: &self.stale,
            descriptors: &self.descriptors,
            described: &described,
            output: output.as_ref(),
            language: shared.language(),
        };
        let mut value = with_archive(item_view::item(&known, summary, item.as_ref()), &self.archived, &summary.id);
        value["owner"] = json!(owned_by(shared, item.as_ref()));
        let (value, _) = shared.stamps.stamp(value);
        Ok(value)
    }
}

fn owned_by(shared: &Shared, item: Option<&domain::workflow::WorkItem>) -> String {
    item.map_or_else(
        || domain::principal::LOCAL_USER.to_owned(),
        |item| orchestrator::owners::of(&shared.backend.store, item),
    )
}

fn stale(shared: &Shared, connections: &[ConnectionRecord]) -> Vec<String> {
    let Some(health) = shared.backend.orchestrator.health() else { return Vec::new() };
    let lost = |record: &&ConnectionRecord| {
        matches!(
            health.state(&domain::connection::ConnectionId::from(record.id.as_str())),
            sync::ConnectionState::NeedsReconnect { .. }
        )
    };
    connections.iter().filter(lost).map(|record| record.id.clone()).collect()
}

struct Origin<'a> {
    sources: Vec<Value>,
    git: Map<String, Value>,
    application: Option<&'a Application>,
    title: Option<String>,
    kind: Option<String>,
}

fn filled(value: &Value) -> Option<String> {
    value.as_str().map(str::trim).filter(|text| !text.is_empty()).map(str::to_owned)
}

fn source(found: &Value) -> ApiResult<Value> {
    let kind = found["kind"].as_str().unwrap_or_default();
    if !KINDS.contains(&kind) {
        return Err(ApiError::bad(said(
            format!("Unknown source {kind}."),
            format!("No conozco el origen {kind}."),
            format!("Não conheço a origem {kind}."),
        )));
    }
    let subject = filled(&found["subject"]).ok_or_else(|| {
        ApiError::bad(said(
            "Give the reference of each source.",
            "Indica la referencia de cada origen.",
            "Diga a referência de cada origem.",
        ))
    })?;
    let mut kept = json!({ "kind": kind, "connection": filled(&found["connection"]), "subject": subject });
    if let Some(repository) = filled(&found["repository"]) {
        kept["repository"] = json!(repository);
    }
    Ok(kept)
}

fn git(body: &Value) -> Map<String, Value> {
    let mut kept = Map::new();
    for (repository, wanted) in body["git"].as_object().into_iter().flatten() {
        let mut entry = Map::new();
        for name in ["base", "head"] {
            if let Some(found) = filled(&wanted[name]) {
                entry.insert(name.to_owned(), json!(found));
            }
        }
        if !entry.is_empty() {
            kept.insert(repository.clone(), Value::Object(entry));
        }
    }
    kept
}

fn code_record(shared: &Shared, source: &Value, application: Option<&Application>) -> ApiResult<ConnectionRecord> {
    let connections = shared.backend.store.connections()?;
    let repositories = application.into_iter().flat_map(|application| &application.repositories);
    let linked = repositories.filter_map(|repository| repository.bound(contracts::CODE)).next();
    match filled(&source["connection"]).or_else(|| linked.map(|code| code.connection.clone())) {
        Some(id) => connections.into_iter().find(|record| record.id == id).ok_or_else(|| {
            ApiError::bad(said(
                "The pull request account is no longer connected.",
                "La cuenta del pull request ya no está conectada.",
                "A conta do pull request não está mais conectada.",
            ))
        }),
        None => serving(&connections, contracts::CODE).cloned().ok_or_else(|| {
            ApiError::bad(said(
                "Connect a code account to start from a pull request.",
                "Conecta una cuenta de código para empezar desde un pull request.",
                "Conecte uma conta de código para começar de um pull request.",
            ))
        }),
    }
}

async fn issue_type(shared: &Shared, source: &Value) -> Option<String> {
    let connections = shared.backend.store.connections().ok()?;
    let record = match filled(&source["connection"]) {
        Some(id) => connections.iter().find(|record| record.id == id),
        None => serving(&connections, contracts::TRACKER),
    }?;
    let args = json!({ "issue": filled(&source["subject"])? });
    let hub = &shared.backend.connectors;
    let found = hub.call(&connection(record), &record.settings, contracts::tracker::ISSUE, &args, "").await.ok()?;
    filled(&found["type"])
}

async fn read_pull(shared: &Shared, record: &ConnectionRecord, subject: &str) -> ApiResult<Value> {
    let key = format!("{}:{}", brand::connectors::MARKER, crate::attachments::random_id()?);
    let args = json!({ "pr": subject });
    let hub = &shared.backend.connectors;
    let found = hub.call(&connection(record), &record.settings, code::PULL_REQUEST, &args, &key).await;
    found.map_err(|error| {
        ApiError::bad(said(
            format!("Could not read pull request {subject}: {}", error.shown()),
            format!("No pude leer el pull request {subject}: {}", error.shown()),
            format!("Não consegui ler o pull request {subject}: {}", error.shown()),
        ))
    })
}

fn pull_repo(found: &Value) -> Option<String> {
    let from_ref = || filled(&found["ref"]).and_then(|whole| whole.split_once('#').map(|(repo, _)| repo.to_owned()));
    filled(&found["repo"]).or_else(from_ref)
}

fn matching<'a>(applications: &'a [Application], repo: &str) -> Option<(&'a Application, &'a Repository)> {
    applications.iter().find_map(|application| {
        let same = |repository: &&Repository| {
            repository.bound(contracts::CODE).is_some_and(|code| code.value.eq_ignore_ascii_case(repo))
        };
        application.repositories.iter().find(same).map(|repository| (application, repository))
    })
}

impl<'a> Origin<'a> {
    fn read(body: &Value, application: Option<&'a Application>) -> ApiResult<Origin<'a>> {
        let listed = body["sources"].as_array().map(Vec::as_slice).unwrap_or_default();
        let mut sources = listed.iter().map(source).collect::<ApiResult<Vec<_>>>()?;
        if let Some(key) = filled(&body["issue"]["key"])
            && !sources.iter().any(|source| source["kind"] == ISSUE)
        {
            let connection = filled(&body["issue"]["connection"]);
            sources.insert(0, json!({ "kind": ISSUE, "connection": connection, "subject": key }));
        }
        Ok(Origin { sources, git: git(body), application, title: None, kind: None })
    }

    fn of(&self, kind: &str) -> Option<&Value> {
        self.sources.iter().find(|source| source["kind"] == kind)
    }

    fn primary(&self) -> &'static str {
        KINDS.into_iter().find(|kind| self.of(kind).is_some()).unwrap_or("text")
    }

    async fn resolve(&mut self, shared: &Shared, applications: &'a [Application]) -> ApiResult<()> {
        for at in 0..self.sources.len() {
            match self.sources[at]["kind"].as_str() {
                Some(PULL_REQUEST) => self.pull(shared, applications, at).await?,
                Some(BRANCH) => self.branch(at)?,
                Some(ISSUE) => self.kind = issue_type(shared, &self.sources[at]).await,
                _ => {}
            }
        }
        self.checked()
    }

    async fn pull(&mut self, shared: &Shared, applications: &'a [Application], at: usize) -> ApiResult<()> {
        let subject = filled(&self.sources[at]["subject"]).unwrap_or_default();
        let record = code_record(shared, &self.sources[at], self.application)?;
        let found = read_pull(shared, &record, &subject).await?;
        let repo = pull_repo(&found).ok_or_else(|| {
            ApiError::bad(said(
                format!("Pull request {subject} does not say which repository it is from."),
                format!("El pull request {subject} no dice de qué repositorio es."),
                format!("O pull request {subject} não diz de qual repositório é."),
            ))
        })?;
        let among = self.application.map_or(applications, std::slice::from_ref);
        let repository = if among.is_empty() {
            String::new()
        } else {
            let (application, repository) = matching(among, &repo).ok_or_else(|| {
                ApiError::bad(said(
                    format!("No application repository matches {repo}."),
                    format!("Ningún repositorio de las aplicaciones corresponde a {repo}."),
                    format!("Nenhum repositório das aplicações corresponde a {repo}."),
                ))
            })?;
            self.application = Some(application);
            repository.id.clone()
        };
        let source = &mut self.sources[at];
        source["connection"] = json!(record.id);
        source["repository"] = json!(repository);
        if let Some(reference) = filled(&found["ref"]) {
            source["subject"] = json!(reference);
        }
        let entry = self.git.entry(repository).or_insert_with(|| json!({}));
        for name in ["base", "head"] {
            if let Some(branch) = filled(&found[name]) {
                entry[name] = json!(branch);
            }
        }
        self.title = self.title.take().or_else(|| filled(&found["title"]));
        Ok(())
    }

    fn branch(&mut self, at: usize) -> ApiResult<()> {
        let only = match self.application.map(|application| application.repositories.as_slice()) {
            None => Some(String::new()),
            Some([only]) => Some(only.id.clone()),
            Some(_) => None,
        };
        let repository = filled(&self.sources[at]["repository"]).or(only).ok_or_else(|| {
            ApiError::bad(said(
                "Say which repository the branch is in.",
                "Indica en qué repositorio está el branch.",
                "Diga em qual repositório está o branch.",
            ))
        })?;
        let subject = filled(&self.sources[at]["subject"]).unwrap_or_default();
        self.sources[at]["repository"] = json!(repository);
        let entry = self.git.entry(repository).or_insert_with(|| json!({}));
        if entry.get("head").is_none() {
            entry["head"] = json!(subject);
        }
        Ok(())
    }

    fn checked(&self) -> ApiResult<()> {
        let Some(application) = self.application else { return Ok(()) };
        let known = |id: &String| application.repositories.iter().any(|repository| &repository.id == id);
        match self.git.keys().find(|id| !known(id)) {
            Some(_) => Err(ApiError::bad(said(
                "One of the chosen repositories is not from that application.",
                "Uno de los repositorios elegidos no es de esa aplicación.",
                "Um dos repositórios escolhidos não é dessa aplicação.",
            ))),
            None => Ok(()),
        }
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
    let described = context.described(shared, &summary.workflow);
    let item = shared.backend.orchestrator.item(id)?;
    let output = item.as_ref().and_then(|item| shared.backend.orchestrator.output_of(item));
    let known = Known {
        connections: &context.connections,
        stale: &context.stale,
        descriptors: &context.descriptors,
        described: &described,
        output: output.as_ref(),
        language: shared.language(),
    };
    let mut value = with_archive(item_view::item(&known, &summary, item.as_ref()), &context.archived, id);
    value["owner"] = json!(owned_by(shared, item.as_ref()));
    let (value, moved) = shared.stamps.stamp(value);
    Ok(moved.then_some(value))
}

pub async fn list(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let space = query.get("application").map(String::as_str).filter(|id| !id.is_empty());
    let within = |item: &Value| space.is_none_or(|id| item["application"]["id"] == id);
    let mine: Vec<Value> =
        all(&ctx)?.into_iter().filter(|item| item["owner"] == person.owner() && within(item)).collect();
    Ok(reply(mine))
}

fn spaced(shared: &Shared, names: &[String], wanted: &str, kind: &str) -> Option<String> {
    let known = names.iter().find(|name| name.as_str() == wanted)?;
    let accepted = shared.backend.orchestrator.accepts(known);
    (accepted.is_empty() || accepted.iter().any(|name| name == kind)).then(|| known.clone())
}

fn workflow(shared: &Shared, body: &Value, origin: &Origin<'_>) -> ApiResult<String> {
    let names = shared.backend.orchestrator.workflows();
    if let Some(wanted) = filled(&body["workflow"]) {
        let known = names.contains(&wanted);
        return known.then_some(wanted.clone()).ok_or_else(|| {
            ApiError::bad(said(
                format!("There is no workflow {wanted}."),
                format!("No existe el workflow {wanted}."),
                format!("Não há o workflow {wanted}."),
            ))
        });
    }
    let kind = origin.primary();
    if let Some(chosen) = origin.application.and_then(|application| spaced(shared, &names, &application.workflow, kind))
    {
        return Ok(chosen);
    }
    let accepting: Vec<&String> = names
        .iter()
        .filter(|name| shared.backend.orchestrator.accepts(name).iter().any(|accepted| accepted == kind))
        .collect();
    let preferred = accepting.iter().find(|name| name.as_str() == DEFAULT_WORKFLOW).or(accepting.first()).copied();
    let fallback = || names.iter().find(|name| *name == DEFAULT_WORKFLOW).or(names.first());
    preferred.or_else(fallback).cloned().ok_or_else(|| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed",
            said("No workflow found.", "Ningún workflow encontrado.", "Nenhum workflow encontrado."),
        )
    })
}

fn title(text: &str, origin: &Origin<'_>) -> String {
    let first: String = text.lines().next().unwrap_or_default().chars().take(MOST_TITLE).collect();
    if !first.is_empty() {
        return first;
    }
    let subject = || origin.of(ISSUE).or_else(|| origin.sources.first()).and_then(|source| filled(&source["subject"]));
    origin.title.clone().or_else(subject).unwrap_or_default()
}

fn input(shared: &Shared, body: &Value, origin: &Origin<'_>) -> ApiResult<Value> {
    let text = body["text"].as_str().map(str::trim).unwrap_or_default();
    if text.is_empty() && origin.sources.is_empty() {
        return Err(ApiError::bad(said(
            "Describe the item or choose a source: issue, pull request or branch.",
            "Describe el ítem o elige un origen: issue, pull request o branch.",
            "Descreva o item ou escolha uma origem: issue, pull request ou branch.",
        )));
    }
    let files = crate::attachments::paths(&shared.backend.attachments(), &body["attachments"]);
    let files: Vec<String> = files.iter().map(|file| file.display().to_string()).collect();
    let mut input =
        json!({ "title": title(text, origin), "text": text, "attachments": files, "sources": origin.sources });
    if !text.is_empty() {
        input["description"] = json!(text);
    }
    if !origin.git.is_empty() {
        input["git"] = Value::Object(origin.git.clone());
    }
    let application = origin.application;
    if let Some(kind) = &origin.kind {
        input["type"] = json!(kind);
    }
    if let Some(issue) = origin.of(ISSUE) {
        input["issue"] = issue["subject"].clone();
        input["connection"] = issue["connection"].clone();
    } else if let Some(chosen) = application.and_then(|application| application.bound(contracts::TRACKER)) {
        input["project"] = json!(chosen.value);
        input["connection"] = json!(chosen.connection);
    }
    if let Some(application) = application {
        input["application"] = crate::applications::snapshot(shared, application)?;
    }
    if let Some(model) = body["model"].as_str().filter(|model| !model.is_empty()) {
        input["model"] = json!(model);
    }
    if let Some(effort) = crate::models::tuning(body)? {
        input["effort"] = json!(effort.cli_value());
    }
    Ok(input)
}

fn chosen<'a>(applications: &'a [Application], body: &Value) -> ApiResult<Option<&'a Application>> {
    let Some(id) = body["application"].as_str().filter(|id| !id.is_empty()) else { return Ok(None) };
    let found = applications.iter().find(|application| application.id == id);
    found.map(Some).ok_or_else(|| {
        ApiError::bad(said(
            "That application no longer exists.",
            "Esa aplicación ya no existe.",
            "Essa aplicação não existe mais.",
        ))
    })
}

pub async fn create(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let applications = crate::applications::all(&ctx, person.owner()).await?;
    let mut origin = Origin::read(&body, chosen(&applications, &body)?)?;
    origin.resolve(&ctx, &applications).await?;
    let mut input = input(&ctx, &body, &origin)?;
    input[domain::principal::OWNER_FIELD] = json!(person.owner());
    if input.get("language").is_none_or(Value::is_null) {
        input["language"] = json!(ctx.backend.store.language_for(person.owner())?.code());
    }
    let workflow = workflow(&ctx, &body, &origin)?;
    let shared = std::sync::Arc::clone(&ctx);
    let models =
        tokio::task::spawn_blocking(move || crate::models::known(&shared)).await.map_err(ApiError::failed)??;
    if models.is_empty() {
        return Err(ApiError::bad(crate::models::none()));
    }
    let id = ctx.backend.orchestrator.create_item(&workflow, input)?;
    let summary = ctx.backend.orchestrator.summary(&id)?.ok_or_else(|| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed",
            said("The item is gone.", "El ítem desapareció.", "O item sumiu."),
        )
    })?;
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
        None => Err(ApiError::missing(said("There is no such item.", "No existe ese ítem.", "Não há esse item."))),
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

pub async fn branches(
    State(ctx): State<Ctx>,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let id = item_id(&id);
    known(&ctx, &id)?;
    let repository = query.get("repository").map_or("", String::as_str);
    Ok(reply(ctx.backend.orchestrator.base_branches(&id, repository).await?))
}

pub async fn base(State(ctx): State<Ctx>, Path(id): Path<String>, Json(body): Json<Value>) -> ApiResult<Response> {
    let id = item_id(&id);
    known(&ctx, &id)?;
    let repository = body["repository"].as_str().unwrap_or_default();
    let base =
        body["base"].as_str().map(str::trim).filter(|base| !base.is_empty()).ok_or_else(|| {
            ApiError::bad(said("Say the base branch.", "Indica la rama base.", "Diga a branch base."))
        })?;
    let from = body["from"].as_str().map(str::trim).filter(|from| !from.is_empty());
    ctx.backend.orchestrator.choose_base(&id, repository, base, from).await?;
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
    let item =
        ctx.backend.orchestrator.item(&id)?.ok_or_else(|| {
            ApiError::missing(said("There is no such item.", "No existe ese ítem.", "Não há esse item."))
        })?;
    let attempt = query.get("attempt").and_then(|text| text.parse::<u32>().ok());
    let view = ctx.backend.orchestrator.step_view(&id, &step, attempt).await?;
    let latest = view.attempts.last().map_or(0, |last| last.n);
    let run = item.steps.iter().rev().find(|run| run.id == step);
    let waiting = view.attempt == latest && run.is_some_and(|run| run.state == StepState::WaitingHuman);
    let connections = ctx.backend.store.connections()?;
    let language = item.input["language"].as_str().and_then(domain::Language::named).unwrap_or(ctx.language());
    let place = Place { connections: &connections, waiting, language };
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
        let place = Place { connections: &connections, waiting: true, language };
        blocks.extend(blocks::normalized(&place, &decided.blocks));
    }
    Ok(reply(json!({
        "item": id.as_str(), "step": step, "attempt": view.attempt, "attempts": attempts, "blocks": blocks,
    })))
}

pub async fn human(
    State(ctx): State<Ctx>,
    Extension(person): Extension<Principal>,
    Path((id, step)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> ApiResult<Response> {
    let id = item_id(&id);
    let action = body["action"].as_str().filter(|action| !action.is_empty());
    let action = action.ok_or_else(|| ApiError::bad(said("Say the action.", "Indica la acción.", "Diga a ação.")))?;
    let (action, payload) = blocks::input(action, body["payload"].clone());
    let by = [person.name.trim(), person.email.trim()].into_iter().find(|name| !name.is_empty()).unwrap_or("human");
    ctx.backend.orchestrator.human_input_by(&id, &step, &action, payload, by)?;
    Ok(done())
}
