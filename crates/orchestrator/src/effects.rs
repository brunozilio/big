use std::process::Stdio;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use domain::connection::{Capability, Connection, ConnectionId, ProviderKey};
use connectors::contracts;
use domain::effect::{Call, Effect, EffectOutcome, EffectRequest, ErrorClass, IdempotencyKey, next_retry};
use sync::Pushed;
use domain::workflow::{ItemId, WorkItem, WorkflowEvent};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::task::AbortHandle;

use crate::agent::{AgentEffect, Failure, workdir};
use crate::events::Wake;
use crate::git;
use crate::matching::matches;
use crate::protocol::{AGENT, CALL, CHILD, Command, EVENT, EXEC, IMPEDIMENT, SLEEP, STATUS, WAIT_HUMAN, WAIT_POLL};
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
        let at = timer(shared, item, &timeout_timer(&launch.key), limit)?;
        tasks.push(fire_at(shared, item, &launch.key, at, EffectOutcome::TimedOut));
    }
    match launch.command.kind.as_str() {
        WAIT_HUMAN => {}
        CHILD => crate::children::start(shared, item, &launch.command, &launch.key)?,
        EVENT => {
            let wait = EventWait {
                item: item.clone(),
                key: launch.key.clone(),
                kind: launch.command.text("kind"),
                filter: launch.command.args["filter"].clone(),
            };
            shared.waits.lock().unwrap_or_else(PoisonError::into_inner).push(wait);
            shared.hurry();
        }
        SLEEP => {
            let ms = launch.command.args["ms"].as_u64().unwrap_or(0);
            let at = timer(shared, item, &launch.key.to_string(), Duration::from_millis(ms))?;
            tasks.push(fire_at(shared, item, &launch.key, at, EffectOutcome::Ok { value: Value::Null }));
        }
        _ => tasks.push(perform(shared, item, launch)),
    }
    Ok(tasks)
}

fn timer(shared: &Shared, item: &ItemId, id: &str, after: Duration) -> crate::error::Result<i64> {
    if let Some(at) = shared.item(item)?.timers.get(id) {
        return Ok(i64::try_from(*at).unwrap_or(i64::MAX));
    }
    let at = now_ms().saturating_add(i64::try_from(after.as_millis()).unwrap_or(i64::MAX));
    shared.append(item, &[WorkflowEvent::TimerSet { id: id.to_owned(), at: u64::try_from(at).unwrap_or(0) }])?;
    Ok(at)
}

fn fire_at(shared: &Arc<Shared>, item: &ItemId, key: &IdempotencyKey, at: i64, outcome: EffectOutcome) -> AbortHandle {
    let (shared_task, item, key) = (Arc::clone(shared), item.clone(), key.clone());
    shared.spawn(async move {
        let left = u64::try_from(at.saturating_sub(now_ms())).unwrap_or(0);
        tokio::time::sleep(Duration::from_millis(left)).await;
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
                if let Err(error) = crate::runner::block(&shared, &item, &launch.key, &reason) {
                    tracing::warn!(%error, "a blocked effect could not be recorded");
                }
                shared.wake(&item, Wake::Blocked(launch.key.seq));
                return;
            }
        };
        if let Err(error) = shared.resolve(&item, &launch.key, outcome) {
            tracing::warn!(%error, "an effect's result could not be kept");
        }
    })
}

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
                return Err(Failure::Blocked(format!("reconecte a conta: {message}")));
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
    let key = format!("{}:{}", brand::COMMAND, launch.key);
    match launch.command.kind.as_str() {
        AGENT => {
            let effect =
                AgentEffect { shared, item, command: &launch.command, key: &launch.key, resume: launch.resume.clone() };
            effect.run().await
        }
        EXEC => exec(shared, &state, &launch.command).await,
        CALL => connector(shared, &launch.command.call(), &key).await,
        WAIT_POLL => poll(shared, &launch.command, &key).await,
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
        Ok(Pushed::NoConnection) => Err(Failure::Blocked(format!("conecte uma conta para {}", contracts::TRACKER))),
        Err(error) => Err(Failure::Class(error.class(), error.to_string())),
    }
}

fn connection_of(shared: &Shared, call: &Call) -> Result<(Connection, Value), Failure> {
    let records =
        shared.store.connections().map_err(|error| Failure::Class(ErrorClass::Transient, error.to_string()))?;
    let found = records.into_iter().find(|record| match &call.connection {
        Some(id) => &record.id == id,
        None => record.capabilities.iter().any(|name| name == &call.capability),
    });
    let Some(record) = found else {
        let wanted = call.connection.clone().unwrap_or_else(|| call.capability.clone());
        return Err(Failure::Blocked(format!("conecte uma conta para {wanted}")));
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

pub async fn connector(shared: &Shared, call: &Call, key: &str) -> Result<Value, Failure> {
    let (connection, settings) = connection_of(shared, call)?;
    shared
        .config.connectors
        .call(&connection, &settings, &call.action, &call.args, key)
        .await
        .map_err(|error| Failure::Class(error.class, error.message))
}

async fn poll(shared: &Shared, command: &Command, key: &str) -> Result<Value, Failure> {
    let every = Duration::from_millis(command.args["every"].as_u64().unwrap_or(30_000).max(10));
    let until = &command.args["until"];
    loop {
        let wait = match connector(shared, &command.call(), key).await {
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

async fn exec(shared: &Shared, item: &WorkItem, command: &Command) -> Result<Value, Failure> {
    let worktree = workdir(shared, &git::branch_name(item.id.as_str())).await?;
    let cwd = command.args["cwd"].as_str().map_or_else(|| worktree.clone(), |dir| worktree.join(dir));
    let mut process = tokio::process::Command::from(platform::shell(&command.text("command")));
    process.current_dir(&cwd).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
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
    Ok(
        json!({ "code": output.status.code().unwrap_or(-1), "output": format!("{stdout}{stderr}"), "stdout": stdout, "stderr": stderr }),
    )
}
