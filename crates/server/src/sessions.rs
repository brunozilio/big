use std::path::PathBuf;
use std::sync::Arc;

use app::SessionId;
use axum::Json;
use axum::extract::{Path, State};
use axum::response::Response;
use domain::AgentAsk;
use serde_json::{Value, json};

use crate::chats::{Message, Notify};
use crate::error::{ApiError, ApiResult, done, json as reply};
use crate::marks::now_ms;
use crate::session_view::{self, Shown};
use crate::state::{Ctx, Shared, held};

pub fn valid(id: &str) -> ApiResult<&str> {
    let allowed = |letter: char| letter.is_ascii_alphanumeric() || letter == '-' || letter == '_';
    Some(id)
        .filter(|id| !id.is_empty() && !id.starts_with('-') && id.chars().all(allowed))
        .ok_or_else(|| ApiError::bad("Isso não é um id de sessão."))
}

struct Row {
    folder: PathBuf,
    title: Option<String>,
}

fn row(shared: &Shared, session: &str) -> ApiResult<Row> {
    let rows = shared.backend.client.sessions().map_err(ApiError::failed)?;
    let found = rows.into_iter().find(|row| row.session.0 == session);
    let found = found.ok_or_else(|| ApiError::missing("Não há essa sessão nesta máquina."))?;
    Ok(Row { folder: found.cwd, title: found.title })
}

fn shown(shared: &Shared, session: &str, folder: &std::path::Path) -> ApiResult<Shown> {
    let records = shared.backend.client.history(&SessionId(session.to_owned()), folder).map_err(ApiError::failed)?;
    Ok(Shown::of(&records))
}

fn kernel_ask(shared: &Shared, session: &str) -> Option<AgentAsk> {
    let asks = shared.backend.orchestrator.asks();
    asks.into_iter().find(|pending| pending.session.as_deref() == Some(session)).map(|pending| pending.ask)
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
    for event in answered {
        match events.iter_mut().find(|known| known["id"] == event["id"]) {
            Some(known) => *known = event,
            None => events.push(event),
        }
    }
    events.sort_by(|left, right| left["at"].as_str().cmp(&right["at"].as_str()));
    Ok(events)
}

pub fn view(shared: &Shared, session: &str) -> ApiResult<Value> {
    let row = row(shared, session)?;
    let shown = shown(shared, session, &row.folder)?;
    let title = title_of(&row, &shown);
    let (events, status, mode, model) = match shared.chats.view(session) {
        Some(going) => going,
        None => {
            let pending = kernel_ask(shared, session).or_else(|| shown.transcript.ask().map(|found| found.ask.clone()));
            let events = session_view::events(&shown, pending.as_ref());
            let status = session_view::status(&shown.transcript, pending.is_some());
            (events, status, shown.mode.clone(), shown.model.clone())
        }
    };
    let events = with_answered(shared, session, events)?;
    let agents = agents(shared, session)?;
    let work = session_view::work(&shown.transcript);
    Ok(json!({
        "id": session, "title": title, "status": status, "mode": mode, "model": model, "agents": agents,
        "events": events, "work": work,
    }))
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

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> ApiResult<T> + Send + 'static) -> ApiResult<T> {
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
    let wanted = wanted.filter(|wanted| *wanted != crate::models::AUTO);
    domain::model::choose_model(&models, wanted)
        .ok_or_else(|| ApiError::bad(crate::models::NONE))
}

fn prompt(shared: &Shared, body: &Value) -> ApiResult<String> {
    let text = body["text"].as_str().map(str::trim).unwrap_or_default().to_owned();
    if text.is_empty() {
        return Err(ApiError::bad("Escreva a mensagem."));
    }
    let files = crate::attachments::paths(&shared.backend.attachments(), &body["attachments"]);
    if files.is_empty() {
        return Ok(text);
    }
    let listed: Vec<String> = files.iter().map(|file| format!("- {}", file.display())).collect();
    Ok(format!("{text}\n\nAnexos:\n{}", listed.join("\n")))
}

fn send_now(ctx: &Ctx, session: &str, body: &Value) -> ApiResult<()> {
    let shared = ctx.as_ref();
    if kernel_ask(shared, session).is_some() {
        return Err(ApiError::busy("O agente espera uma resposta; responda o pedido aberto."));
    }
    let row = row(shared, session)?;
    let shown = shown(shared, session, &row.folder)?;
    if !shared.chats.running(session) && session_view::status(&shown.transcript, false) == "running" {
        return Err(ApiError::busy("O agente ainda está trabalhando nesta sessão."));
    }
    let text = prompt(shared, body)?;
    let model = chosen_model(shared, body["model"].as_str())?;
    let effort = crate::models::tuning(body)?;
    let nudged = Arc::clone(ctx);
    let notify: Notify = Arc::new(move |session: &str| nudged.follow(session));
    let message = Message { session, folder: row.folder, text: &text, model, effort, shown };
    shared.chats.send(&shared.backend.client, message, notify)
}

pub async fn send(State(ctx): State<Ctx>, Path(id): Path<String>, Json(body): Json<Value>) -> ApiResult<Response> {
    let id = valid(&id)?.to_owned();
    let shared = Arc::clone(&ctx);
    blocking(move || send_now(&shared, &id, &body)).await?;
    Ok(done())
}

fn answer_now(shared: &Shared, session: &str, event: &str, response: &Value) -> ApiResult<()> {
    let from_kernel = kernel_ask(shared, session).filter(|ask| ask.id.0 == event);
    let ask = from_kernel.clone().or_else(|| shared.chats.pending(session).filter(|ask| ask.id.0 == event));
    let ask = ask.ok_or_else(|| ApiError::missing("Esse pedido já foi respondido ou não existe."))?;
    let decision = session_view::decision(&ask.kind, response)
        .map(|decision| if from_kernel.is_some() { session_view::handed_off(decision) } else { decision })
        .ok_or_else(|| ApiError::bad("Essa resposta não serve a esse pedido."))?;
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
    let event = body["event"].as_str().map(str::to_owned).ok_or_else(|| ApiError::bad("Diga qual pedido."))?;
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
    Err(ApiError::busy("Esta sessão é de uma etapa em andamento; cancele o item para pará-la."))
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
        let mut followed = held(&shared.followed);
        let entry = followed.entry(session.clone()).or_default();
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
        if entry.status == "idle" && entry.wanted.elapsed() > FORGET_AFTER {
            followed.remove(session);
        }
    }
}
