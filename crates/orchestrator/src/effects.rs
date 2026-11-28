use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use connectors::contracts;
use domain::Language;
use domain::connection::{Capability, Connection, ConnectionId, ProviderKey};
use domain::effect::{Call, Effect, EffectOutcome, EffectRequest, ErrorClass, IdempotencyKey, next_retry};
use domain::origin::{Attribution, Origin};
use domain::workflow::{ItemId, WorkItem, WorkflowEvent};
use serde_json::{Value, json};
use sync::Pushed;
use tokio::io::AsyncWriteExt;
use tokio::task::AbortHandle;

use crate::agent::{AgentEffect, Failure, blocking, worktree};
use crate::events::Wake;
use crate::git;
use crate::matching::{event_matches, matches};
use crate::policy::Gated;
use crate::protocol::{
    AGENT, CALL, CHILD, Command, EVENT, EXEC, IMPEDIMENT, SCRIPT, SLEEP, STATUS, WAIT_HUMAN, WAIT_POLL,
};
use crate::shared::{EventWait, Shared, now_ms, timeout_timer};

pub struct Launch {
    pub command: Command,
    pub key: IdempotencyKey,
    pub resume: Option<String>,
}

pub fn launch(shared: &Arc<Shared>, item: &ItemId, launch: Launch) -> crate::error::Result<Vec<AbortHandle>> {
    let mut tasks = Vec::new();
    if let Some(limit) = launch
        .command
        .timeout
        .map(Duration::from_millis)
        .or_else(|| shared.config.limits.timeouts.of(&launch.command.kind))
    {
        let id = timeout_timer(&launch.key);
        let at = timer(shared, item, &id, limit)?;
        tasks.push(fire_at(shared, item, (&launch.key, id), at, EffectOutcome::TimedOut));
    }
    match launch.command.kind.as_str() {
        WAIT_HUMAN => crate::chores::recap(shared, item, &launch.key.to_string()),
        CHILD => crate::children::start(shared, item, &launch.command, &launch.key)?,
        EVENT => {
            let wait = EventWait {
                item: item.clone(),
                key: launch.key.clone(),
                kind: launch.command.text("kind"),
                filter: launch.command.args["filter"].clone(),
            };
            match seen_before(shared, &wait)? {
                Some(value) => {
                    shared.resolve(item, &launch.key, EffectOutcome::Ok { value })?;
                }
                None => {
                    shared.waits.lock().unwrap_or_else(PoisonError::into_inner).push(wait);
                    shared.hurry();
                }
            }
        }
        SLEEP => {
            let ms = launch.command.args["ms"].as_u64().unwrap_or(0);
            let id = launch.key.to_string();
            let at = timer(shared, item, &id, Duration::from_millis(ms))?;
            tasks.push(fire_at(shared, item, (&launch.key, id), at, EffectOutcome::Ok { value: Value::Null }));
        }
        _ => tasks.push(perform(shared, item, launch)),
    }
    Ok(tasks)
}

fn seen_before(shared: &Shared, wait: &EventWait) -> crate::error::Result<Option<Value>> {
    let state = shared.item(&wait.item)?;
    let ledger = crate::ledger::Ledger::load(&shared.store, &wait.item)?;
    let since = ledger.requested_at.get(&wait.key.seq).copied().unwrap_or(i64::MAX);
    let mut seen: Vec<_> = state.observed.values().filter(|seen| seen.kind == wait.kind && seen.at >= since).collect();
    seen.sort_by_key(|seen| seen.at);
    Ok(seen.into_iter().find_map(|seen| {
        let at = jiff::Timestamp::from_millisecond(seen.at).map(|at| at.to_string()).unwrap_or_default();
        let value = json!({
            "id": seen.id, "connection": seen.connection, "capability": seen.kind.split('.').next(),
            "kind": seen.kind, "subject": seen.subject, "data": seen.data, "at": at,
        });
        event_matches(&wait.filter, &value).then_some(value)
    }))
}

fn timer(shared: &Shared, item: &ItemId, id: &str, after: Duration) -> crate::error::Result<i64> {
    if let Some(at) = shared.item(item)?.timers.get(id) {
        return Ok(i64::try_from(*at).unwrap_or(i64::MAX));
    }
    let at = now_ms().saturating_add(i64::try_from(after.as_millis()).unwrap_or(i64::MAX));
    shared.append(item, &[WorkflowEvent::TimerSet { id: id.to_owned(), at: u64::try_from(at).unwrap_or(0) }])?;
    Ok(at)
}

fn fire_at(
    shared: &Arc<Shared>,
    item: &ItemId,
    (key, id): (&IdempotencyKey, String),
    at: i64,
    outcome: EffectOutcome,
) -> AbortHandle {
    let (shared_task, item, key) = (Arc::clone(shared), item.clone(), key.clone());
    shared.spawn(async move {
        let left = u64::try_from(at.saturating_sub(now_ms())).unwrap_or(0);
        tokio::time::sleep(Duration::from_millis(left)).await;
        let set = shared_task.item(&item).is_ok_and(|state| state.timers.contains_key(&id));
        if !set {
            return;
        }
        if let Err(error) = shared_task.resolve(&item, &key, outcome) {
            tracing::warn!(%error, "a timer could not resolve its effect");
        }
    })
}

fn perform(shared: &Arc<Shared>, item: &ItemId, launch: Launch) -> AbortHandle {
    let (task_shared, item) = (Arc::clone(shared), item.clone());
    shared.spawn(async move {
        let shared = task_shared;
        let outcome = match attempts(&shared, &item, &launch).await {
            Ok(value) => EffectOutcome::Ok { value },
            Err(Failure::Class(class, message)) => EffectOutcome::Failed { class, message },
            Err(Failure::Blocked(reason)) => {
                shared.wake(&item, Wake::Blocked(launch.key.seq));
                if let Err(error) = crate::runner::block(&shared, &item, &launch.key, &reason) {
                    tracing::warn!(%error, "a blocked effect could not be recorded");
                }
                return;
            }
        };
        if let Err(error) = shared.resolve(&item, &launch.key, outcome) {
            tracing::warn!(%error, "an effect's result could not be kept");
        }
    })
}

pub const NEEDS_CONNECTION: &str = "connect an account for ";
pub const NEEDS_RECONNECT: &str = "reconnect the account: ";

fn roll() -> f64 {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    f64::from(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])) / f64::from(u32::MAX)
}

async fn attempts(shared: &Arc<Shared>, item: &ItemId, launch: &Launch) -> Result<Value, Failure> {
    let policy = launch.command.retry(&shared.config.limits.retry);
    prepare(shared, item, &launch.command).map_err(|error| Failure::permanent(error.to_string()))?;
    let mut attempt = 1;
    loop {
        let class = match once(shared, item, launch).await {
            Ok(value) => return Ok(value),
            Err(Failure::Class(ErrorClass::Auth, message)) => {
                return Err(Failure::Blocked(format!("{NEEDS_RECONNECT}{message}")));
            }
            Err(Failure::Class(class, message)) => (class, message),
            Err(blocked) => return Err(blocked),
        };
        let Some(delay) = next_retry(&policy, attempt, &class.0) else { return Err(Failure::Class(class.0, class.1)) };
        tokio::time::sleep(policy.jittered(delay, roll())).await;
        attempt += 1;
    }
}

fn prepare(shared: &Shared, item: &ItemId, command: &Command) -> crate::error::Result<()> {
    match command.kind.as_str() {
        STATUS => shared.append(item, &[WorkflowEvent::StatusSet(command.text("name"))]).map(|_| ()),
        IMPEDIMENT => {
            let on = command.args["on"].as_bool().unwrap_or(false);
            let reason = command.args["reason"].as_str().map(str::to_owned);
            shared.append(item, &[WorkflowEvent::ImpedimentSet { on, reason }]).map(|_| ())
        }
        _ => Ok(()),
    }
}

async fn once(shared: &Arc<Shared>, item: &ItemId, launch: &Launch) -> Result<Value, Failure> {
    let state = shared.item(item).map_err(|error| Failure::permanent(error.to_string()))?;
    let key = format!("{}:{}", brand::connectors::MARKER, launch.key);
    let owner = crate::owners::of(&shared.store, &state);
    let charged = Attribution::of_step(launch.key.item.clone(), launch.key.step.clone(), Origin::Step);
    let language = crate::agent::language_of(shared, &state.input);
    let who = Who { owner: &owner, language, charged: &charged };
    match launch.command.kind.as_str() {
        AGENT => {
            let effect =
                AgentEffect {
                shared,
                item,
                command: &launch.command,
                key: &launch.key,
                resume: launch.resume.clone(),
                charge: std::sync::OnceLock::new(),
            };
            effect.run().await
        }
        EXEC => exec(shared, &state, &launch.command).await,
        SCRIPT => crate::scripts::run(shared, &state, &launch.command).await,
        CALL if launch.command.call().capability == crate::links::CAPABILITY => {
            crate::links::call(shared, item, &launch.command.call())
        }
        CALL => {
            let gated = Gated { shared, item: &state, key: &launch.key, owner: &owner };
            call(who, &gated, &launch.command, &key).await
        }
        WAIT_POLL => {
            Gated { shared, item: &state, key: &launch.key, owner: &owner }.check(&launch.command.call()).await?;
            poll(shared, who, &launch.command, &key).await
        }
        STATUS | IMPEDIMENT => push(shared, &state, &launch.command, &launch.key).await,
        other => Err(Failure::permanent(format!("the kernel does not know the command {other}"))),
    }
}

async fn push(shared: &Shared, item: &WorkItem, command: &Command, key: &IdempotencyKey) -> Result<Value, Failure> {
    let mut request = EffectRequest::new(key.clone(), command.effect());
    if let Effect::ImpedimentSet { on: false, .. } = request.effect {
        request.effect = Effect::ImpedimentSet { on: false, reason: None };
    }
    match sync::push(&shared.config.connectors, &shared.store, item, &request).await {
        Ok(Pushed::Done(_) | Pushed::NoIssue | Pushed::NotSynced) => Ok(Value::Null),
        Ok(Pushed::NoConnection) => Err(Failure::Blocked(format!("{NEEDS_CONNECTION}{}", contracts::TRACKER))),
        Err(error) if matches!(request.effect, Effect::StatusSet { .. }) && error.class() == ErrorClass::Permanent => {
            tracing::warn!(%error, "the tracker refused the status; the item keeps it here");
            Ok(json!({ "unsynced": error.to_string() }))
        }
        Err(error) => Err(Failure::Class(error.class(), error.to_string())),
    }
}

pub fn connection_of(shared: &Shared, owner: &str, call: &Call) -> Result<(Connection, Value), Failure> {
    let records =
        shared.store.connections_of(owner).map_err(|error| Failure::Class(ErrorClass::Transient, error.to_string()))?;
    let found = records.into_iter().find(|record| match &call.connection {
        Some(id) => &record.id == id,
        None => record.capabilities.iter().any(|name| name == &call.capability),
    });
    let Some(record) = found else {
        let wanted = call.connection.clone().unwrap_or_else(|| call.capability.clone());
        return Err(Failure::Blocked(format!("{NEEDS_CONNECTION}{wanted}")));
    };
    let connection = Connection {
        id: ConnectionId::from(record.id.as_str()),
        provider: ProviderKey::from(record.provider.as_str()),
        label: record.label,
        account: record.account.unwrap_or_default(),
        capabilities: record.capabilities.iter().map(|name| Capability::from(name.as_str())).collect(),
    };
    Ok((connection, record.settings))
}

#[derive(Debug, Clone, Copy)]
pub struct Who<'a> {
    pub owner: &'a str,
    pub language: Language,
    pub charged: &'a Attribution,
}

async fn call(who: Who<'_>, gated: &Gated<'_>, command: &Command, key: &str) -> Result<Value, Failure> {
    let shared = gated.shared;
    let mut call = command.call();
    let store = crate::kept::store(&shared.config.data);
    let mut args = std::mem::take(&mut call.args);
    call.args = blocking(move || {
        crate::kept::expand(&store, &mut args);
        args
    })
    .await?;
    if command.optional && call.connection.is_none() {
        match connection_of(shared, who.owner, &call) {
            Err(Failure::Blocked(_)) => return Ok(Value::Null),
            Err(other) => return Err(other),
            Ok(_) => {}
        }
    }
    gated.check(&call).await?;
    connector(shared, who, &call, key).await
}

pub async fn connector(shared: &Shared, who: Who<'_>, call: &Call, key: &str) -> Result<Value, Failure> {
    let (connection, settings) = connection_of(shared, who.owner, call)?;
    let hub = &shared.config.connectors;
    let called = hub.call_for(Some(who.charged), &connection, &settings, &call.action, &call.args, key);
    called.await.map_err(|error| match error.blocker() {
        Some(reason) => Failure::Blocked(hub.told(&connection.provider, who.language, reason)),
        None => Failure::Class(error.class, error.message),
    })
}

async fn poll(shared: &Shared, who: Who<'_>, command: &Command, key: &str) -> Result<Value, Failure> {
    let every = Duration::from_millis(command.args["every"].as_u64().unwrap_or(30_000).max(10));
    let until = &command.args["until"];
    loop {
        let wait = match connector(shared, who, &command.call(), key).await {
            Ok(value) if matches(until, &value) => return Ok(value),
            Ok(_) => every,
            Err(Failure::Class(ErrorClass::Permanent, message)) => return Err(Failure::permanent(message)),
            Err(Failure::Class(ErrorClass::Auth, message)) => return Err(Failure::Class(ErrorClass::Auth, message)),
            Err(Failure::Class(ErrorClass::RateLimited { retry_after }, _)) => retry_after.max(every),
            Err(Failure::Class(_, _)) => every,
            Err(blocked) => return Err(blocked),
        };
        tokio::time::sleep(wait).await;
    }
}

pub(crate) async fn worktree_of(shared: &Shared, item: &WorkItem, command: &Command) -> Result<PathBuf, Failure> {
    let wanted = command.args["repository"].as_str().unwrap_or_default();
    let checkouts = shared.checkouts(item);
    let chosen = checkouts.iter().find(|checkout| checkout.id == wanted).or(checkouts.first()).cloned();
    let checkout = chosen.ok_or_else(|| Failure::permanent("the item has no repository"))?;
    let (branch, base) = (git::head_of(item, &checkout.id), git::base_of(item, &checkout.id));
    Ok(worktree(shared, checkout, &branch, base).await?.path)
}

async fn exec(shared: &Shared, item: &WorkItem, command: &Command) -> Result<Value, Failure> {
    let worktree = worktree_of(shared, item, command).await?;
    let cwd = command.args["cwd"].as_str().map_or_else(|| worktree.clone(), |dir| worktree.join(dir));
    let mut process = tokio::process::Command::from(platform::shell(&command.text("command")));
    process.current_dir(&cwd).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    process.envs(crate::application::environment(&shared.store, &item.input));
    if let Some(env) = command.args["env"].as_object() {
        process.envs(
            env.iter()
                .map(|(key, value)| (key.clone(), value.as_str().map_or_else(|| value.to_string(), str::to_owned))),
        );
    }
    let input = command.args["input"].as_str().map(str::to_owned);
    process.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = process
        .spawn()
        .map_err(|error| Failure::Class(ErrorClass::Transient, format!("the command could not start: {error}")))?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes()).await;
    }
    let output =
        child.wait_with_output().await.map_err(|error| Failure::Class(ErrorClass::Transient, error.to_string()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let mut answer = json!({ "code": output.status.code().unwrap_or(-1), "output": format!("{stdout}{stderr}"), "stdout": stdout, "stderr": stderr });
    if command.args["keep"].is_array() {
        let (store, wanted) = (crate::kept::store(&shared.config.data), command.args["keep"].clone());
        let kept = blocking(move || crate::kept::keep(&store, &worktree, &wanted)).await?;
        answer["kept"] = json!(kept);
    }
    Ok(answer)
}
