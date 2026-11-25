use std::path::PathBuf;
use std::sync::Arc;

use app::SessionId;
use axum::Json;
use axum::extract::{Path, State};
use axum::response::Response;
use domain::AgentAsk;
use serde_json::{Value, json};

use crate::chats::{Message, Notify};
use crate::error::{ApiError, ApiResult, done, json as reply, said};
use crate::marks::now_ms;
use crate::session_view::{self, Shown};
use crate::state::{Ctx, Shared, held};

pub fn valid(id: &str) -> ApiResult<&str> {
    let allowed = |letter: char| letter.is_ascii_alphanumeric() || letter == '-' || letter == '_';
    Some(id).filter(|id| !id.is_empty() && !id.starts_with('-') && id.chars().all(allowed)).ok_or_else(|| {
        ApiError::bad(said("That is not a session id.", "Eso no es un id de sesión.", "Isso não é um id de sessão."))
    })
}

struct Row {
    folder: PathBuf,
    title: Option<String>,
}

fn row(shared: &Shared, session: &str) -> ApiResult<Row> {
    let rows = shared.backend.client.sessions().map_err(ApiError::failed)?;
    let found = rows.into_iter().find(|row| row.session.0 == session);
    let found = found.ok_or_else(|| {
        ApiError::missing(said(
            "There is no such session on this machine.",
            "No existe esa sesión en esta máquina.",
            "Não há essa sessão nesta máquina.",
        ))
    })?;
    Ok(Row { folder: found.cwd, title: found.title })
}

pub fn folder(shared: &Shared, session: &str) -> ApiResult<PathBuf> {
    row(shared, session).map(|row| row.folder)
}

fn shown(shared: &Shared, session: &str, folder: &std::path::Path) -> ApiResult<Shown> {
    let records = shared.backend.client.history(&SessionId(session.to_owned()), folder).map_err(ApiError::failed)?;
    Ok(Shown::of(&records))
}

fn kernel_asks(shared: &Shared, session: &str) -> Vec<AgentAsk> {
    let asks = shared.backend.orchestrator.asks();
    asks.into_iter().filter(|pending| pending.session.as_deref() == Some(session)).map(|pending| pending.ask).collect()
}

fn title_of(row: &Row, shown: &Shown) -> String {
    let first = || shown.transcript.first_prompt().map(|text| text.lines().next().unwrap_or_default().to_owned());
    let title = row.title.clone().or_else(first).unwrap_or_default();
    let title: String = title.chars().take(120).collect();
    if title.trim().is_empty() { "Conversa".to_owned() } else { title }
}

fn with_answered(shared: &Shared, session: &str, mut events: Vec<Value>) -> ApiResult<Vec<Value>> {
    let mut answered = shared.backend.orchestrator.answered(session)?;
    answered.extend(held(&shared.answered).get(session).cloned().unwrap_or_default());
    session_view::merge_answered(&mut events, answered);
    Ok(events)
}

pub fn watched(shared: &Shared, session: &str, event: &app::EngineEvent) {
    let load = || row(shared, session).and_then(|row| shown(shared, session, &row.folder)).unwrap_or_default();
    if shared.watched.see(session, event, load) {
        shared.follow(session);
    }
}

pub fn view(shared: &Shared, session: &str) -> ApiResult<Value> {
    let row = row(shared, session)?;
    if let Some(live) = shared.watched.with(session, |shown| rendered(shared, session, &row, shown)) {
        return live;
    }
    let shown = shown(shared, session, &row.folder)?;
    rendered(shared, session, &row, &shown)
}

fn rendered(shared: &Shared, session: &str, row: &Row, shown: &Shown) -> ApiResult<Value> {
    let title = title_of(row, shown);
    let (events, status, mode, model) = match shared.chats.view(session) {
        Some(going) => going,
        None => {
            let mut pending = kernel_asks(shared, session);
            if pending.is_empty() {
                pending.extend(shown.transcript.ask().map(|found| found.ask.clone()));
            }
            let events = session_view::events(shown, &pending);
            let status = session_view::status(&shown.transcript, !pending.is_empty());
            let orphaned = status == "running" && !shared.backend.orchestrator.runs_session(session).unwrap_or(true);
            (events, if orphaned { "idle" } else { status }, shown.mode.clone(), shown.model.clone())
        }
    };
    let mut events = with_answered(shared, session, events)?;
    let agents = agents(shared, session)?;
    let jobs = session_view::Jobs::of(&shown.transcript, live_jobs(shared, session));
    jobs.mark(&mut events);
    let work = session_view::work(&shown.transcript, &jobs);
    Ok(json!({
        "id": session, "title": title, "status": status, "mode": mode, "model": model, "agents": agents,
        "events": events, "work": work, "suggestion": shared.chats.suggestion(session),
    }))
}

fn live_jobs(shared: &Shared, session: &str) -> Option<Vec<app::BackgroundJob>> {
    let orchestrator = &shared.backend.orchestrator;
    if let Some(jobs) = orchestrator.background_jobs(session) {
        return Some(
            jobs.into_iter().map(|(id, command, running)| app::BackgroundJob { id, command, running }).collect(),
        );
    }
    if orchestrator.runs_session(session).unwrap_or(false) {
        return None;
    }
    shared.backend.client.background_jobs().ok()
}

fn stop_live_job(shared: &Shared, session: &str, job: &str) -> ApiResult<()> {
    match shared.backend.orchestrator.stop_background(session, job) {
        Some(true) => Ok(()),
        Some(false) => Err(ApiError::failed(format!("no background job {job} is running"))),
        None => shared.backend.client.stop_background(job).map_err(ApiError::failed),
    }
}

fn agents(shared: &Shared, session: &str) -> ApiResult<Vec<Value>> {
    let cards = shared.backend.orchestrator.session_agents(session)?;
    Ok(cards
        .into_iter()
        .map(|card| {
            let avatar = match (card.shape, card.color) {
                (Some(shape), Some(color)) => json!({ "kind": "agent", "shape": shape, "color": color }),
                _ => Value::Null,
            };
            json!({ "id": card.id, "name": card.name, "avatar": avatar })
        })
        .collect())
}

pub async fn blocking<T: Send + 'static>(work: impl FnOnce() -> ApiResult<T> + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(work).await.map_err(ApiError::failed)?
}

pub async fn open(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let id = valid(&id)?.to_owned();
    let shared = Arc::clone(&ctx);
    let session = id.clone();
    let value = blocking(move || view(&shared, &session)).await?;
    ctx.follow(&id);
    Ok(reply(value))
}

fn chosen_model(shared: &Shared, wanted: Option<&str>) -> ApiResult<app::Model> {
    let models = shared.backend.client.models().map_err(ApiError::failed)?;
    domain::model::choose_model(&models, wanted).ok_or_else(|| ApiError::bad(crate::models::none()))
}

fn prompt(shared: &Shared, body: &Value) -> ApiResult<String> {
    let text = body["text"].as_str().map(str::trim).unwrap_or_default().to_owned();
    if text.is_empty() {
        return Err(ApiError::bad(said("Write the message.", "Escribe el mensaje.", "Escreva a mensagem.")));
    }
    let files = crate::attachments::paths(&shared.backend.attachments(), &body["attachments"]);
    if files.is_empty() {
        return Ok(text);
    }
    let listed: Vec<String> = files.iter().map(|file| format!("- {}", file.display())).collect();
    Ok(format!("{text}\n\nAttachments:\n{}", listed.join("\n")))
}

fn send_now(ctx: &Ctx, session: &str, body: &Value) -> ApiResult<()> {
    let shared = ctx.as_ref();
    if !kernel_asks(shared, session).is_empty() {
        return Err(ApiError::busy(said(
            "The agent is waiting for an answer; answer the open request.",
            "El agente espera una respuesta; responde la solicitud abierta.",
            "O agente espera uma resposta; responda o pedido aberto.",
        )));
    }
    let text = prompt(shared, body)?;
    if shared.chats.steer(session, &text)? || shared.backend.orchestrator.steer(session, &text) {
        return Ok(());
    }
    let row = row(shared, session)?;
    let shown = shown(shared, session, &row.folder)?;
    if !shared.chats.running(session) && session_view::status(&shown.transcript, false) == "running" {
        return Err(ApiError::busy(said(
            "The agent is still working in this session.",
            "El agente todavía está trabajando en esta sesión.",
            "O agente ainda está trabalhando nesta sessão.",
        )));
    }
    let model = chosen_model(shared, body["model"].as_str())?;
    let effort = crate::models::tuning(body)?;
    let nudged = Arc::clone(ctx);
    let notify: Notify = Arc::new(move |session: &str| nudged.follow(session));
    let agent = shared.backend.orchestrator.session_agent(session)?;
    let mode = session_view::free_mode(agent.as_ref(), &shown);
    let style = shared.backend.store.output_style();
    let language = shared.backend.store.language().ok();
    let message =
        Message { session, folder: row.folder, text: &text, model, effort, agent, mode, style, language, shown };
    shared.chats.send(&shared.backend.client, message, notify)
}

fn side_now(ctx: &Ctx, session: &str, body: &Value) -> ApiResult<Value> {
    let shared = ctx.as_ref();
    let text = body["text"].as_str().map(str::trim).filter(|text| !text.is_empty());
    let text = text
        .ok_or_else(|| ApiError::bad(said("Write the question.", "Escribe la pregunta.", "Escreva a pergunta.")))?
        .to_owned();
    let row = row(shared, session)?;
    let model = chosen_model(shared, body["model"].as_str())?;
    let agent = shared.backend.orchestrator.session_agent(session)?;
    let message = Message {
        session,
        folder: row.folder,
        text: &text,
        model,
        effort: None,
        agent,
        mode: None,
        style: None,
        language: shared.backend.store.language().ok(),
        shown: Shown::default(),
    };
    let answer = crate::chats::Chats::side(&shared.backend.client, &message)?;
    Ok(json!({ "answer": answer }))
}

pub async fn side(State(ctx): State<Ctx>, Path(id): Path<String>, Json(body): Json<Value>) -> ApiResult<Response> {
    let id = valid(&id)?.to_owned();
    let shared = Arc::clone(&ctx);
    let answered = blocking(move || side_now(&shared, &id, &body)).await?;
    Ok(reply(answered))
}

pub async fn send(State(ctx): State<Ctx>, Path(id): Path<String>, Json(body): Json<Value>) -> ApiResult<Response> {
    let id = valid(&id)?.to_owned();
    let shared = Arc::clone(&ctx);
    blocking(move || send_now(&shared, &id, &body)).await?;
    Ok(done())
}

fn answer_now(shared: &Shared, session: &str, event: &str, response: &Value) -> ApiResult<()> {
    let from_kernel = kernel_asks(shared, session).into_iter().find(|ask| ask.id.0 == event);
    let ask = from_kernel.clone().or_else(|| shared.chats.pending(session).filter(|ask| ask.id.0 == event));
    let ask = ask.ok_or_else(|| {
        ApiError::missing(said(
            "That request was already answered or does not exist.",
            "Esa solicitud ya fue respondida o no existe.",
            "Esse pedido já foi respondido ou não existe.",
        ))
    })?;
    let decision = session_view::decision(&ask.kind, response)
        .map(|decision| if from_kernel.is_some() { session_view::handed_off(decision) } else { decision })
        .ok_or_else(|| {
            ApiError::bad(said(
                "That answer does not fit that request.",
                "Esa respuesta no sirve para esa solicitud.",
                "Essa resposta não serve a esse pedido.",
            ))
        })?;
    let resolved = session_view::resolved(session_view::ask_event(&ask, now_ms()), response);
    match from_kernel {
        Some(_) => shared.backend.orchestrator.answer_ask_with(&ask.id, &decision, resolved.clone())?,
        None => {
            shared.chats.answer(session, event, decision)?;
            held(&shared.answered).entry(session.to_owned()).or_default().push(resolved.clone());
        }
    }
    shared.live.session_event(session, resolved);
    Ok(())
}

pub async fn respond(State(ctx): State<Ctx>, Path(id): Path<String>, Json(body): Json<Value>) -> ApiResult<Response> {
    let id = valid(&id)?.to_owned();
    let event = body["event"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ApiError::bad(said("Say which request.", "Indica qué solicitud.", "Diga qual pedido.")))?;
    let shared = Arc::clone(&ctx);
    let session = id.clone();
    blocking(move || answer_now(&shared, &session, &event, &body["response"])).await?;
    ctx.follow(&id);
    Ok(done())
}

pub async fn stop(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let id = valid(&id)?;
    if ctx.chats.stop(id)? {
        return Ok(done());
    }
    Err(ApiError::busy(said(
        "This session belongs to a running step; cancel the item to stop it.",
        "Esta sesión es de una etapa en curso; cancela el ítem para detenerla.",
        "Esta sessão é de uma etapa em andamento; cancele o item para pará-la.",
    )))
}

pub async fn stop_job(State(ctx): State<Ctx>, Path((id, job)): Path<(String, String)>) -> ApiResult<Response> {
    let id = valid(&id)?.to_owned();
    let shared = Arc::clone(&ctx);
    blocking(move || {
        let work = &view(&shared, &id)?["work"];
        let listed =
            work["background"].as_array().is_some_and(|all| all.iter().any(|known| known["id"] == job.as_str()));
        if !listed || work["stoppable"] != true {
            return Err(ApiError::missing(said(
                "This background job is not running in this session.",
                "Este trabajo en segundo plano no está corriendo en esta sesión.",
                "Este job em segundo plano não está rodando nesta sessão.",
            )));
        }
        stop_live_job(&shared, &id, &job)?;
        refresh(&shared, std::slice::from_ref(&id));
        Ok(())
    })
    .await?;
    Ok(done())
}

pub async fn follow_loop(ctx: Ctx, mut nudges: tokio::sync::mpsc::UnboundedReceiver<String>) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        let wanted: Vec<String> = tokio::select! {
            _ = tick.tick() => held(&ctx.followed).keys().cloned().collect(),
            nudged = nudges.recv() => match nudged {
                Some(session) => vec![session],
                None => return,
            },
        };
        let shared = Arc::clone(&ctx);
        let _ = tokio::task::spawn_blocking(move || refresh(&shared, &wanted)).await;
    }
}

const FORGET_AFTER: std::time::Duration = std::time::Duration::from_secs(1800);

fn refresh(shared: &Shared, sessions: &[String]) {
    for session in sessions {
        let Ok(value) = view(shared, session) else { continue };
        let events = value["events"].as_array().cloned().unwrap_or_default();
        let status = value["status"].as_str().unwrap_or("idle").to_owned();
        let suggestion = value["suggestion"].as_str().map(str::to_owned);
        let mut followed = held(&shared.followed);
        let entry = followed.entry(session.clone()).or_default();
        let shown: std::collections::HashSet<String> =
            events.iter().filter_map(|event| event["id"].as_str()).map(str::to_owned).collect();
        let gone: Vec<String> = entry.seen.keys().filter(|id| !shown.contains(*id)).cloned().collect();
        for id in gone {
            entry.seen.remove(&id);
            shared.live.session_event_removed(session, &id);
        }
        for event in events {
            let id = event["id"].as_str().unwrap_or_default().to_owned();
            let signature = event.to_string();
            if entry.seen.get(&id) != Some(&signature) {
                entry.seen.insert(id, signature);
                shared.live.session_event(session, event);
            }
        }
        if entry.status != status {
            shared.live.session_status(session, &status);
            entry.status = status;
        }
        let work = value["work"].to_string();
        if entry.work != work {
            shared.live.session_work(session, value["work"].clone());
            entry.work = work;
        }
        if entry.suggestion != suggestion {
            shared.live.session_suggestion(session, suggestion.as_deref());
            entry.suggestion = suggestion;
        }
        if entry.status == "idle" && entry.wanted.elapsed() > FORGET_AFTER {
            followed.remove(session);
        }
    }
}
