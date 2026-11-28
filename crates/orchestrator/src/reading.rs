use std::time::Duration;

use domain::effect::{Call, ErrorClass};
use domain::redact::Known;
use serde_json::Value;

use crate::agent::Failure;
use crate::effects::{Who, connection_of};
use crate::matching::matches;
use crate::policy::Gated;
use crate::protocol::Command;
use crate::shared::Shared;

pub async fn connector(shared: &Shared, who: Who<'_>, call: &Call, key: &str) -> Result<Value, Failure> {
    labelled(shared, who, call, key).await.map(|read| read.value)
}

pub(crate) async fn labelled(shared: &Shared, who: Who<'_>, call: &Call, key: &str) -> Result<connectors::Labelled, Failure> {
    let (connection, settings) = connection_of(shared, who.owner, who.space, call)?;
    let hub = &shared.config.connectors;
    let called = hub.call_for(Some(who.charged), &connection, &settings, &call.action, &call.args, key);
    let value = called.await.map_err(|error| match error.blocker() {
        Some(reason) => Failure::Blocked(hub.told(&connection.provider, who.language, reason)),
        None => Failure::Class(error.class, error.message),
    })?;
    Ok(hub.labelled(&connection, &call.action, value, &known(shared, who.space)))
}

fn known(shared: &Shared, input: &Value) -> Known {
    let live = crate::application::live(&shared.store, input);
    let checkouts = crate::repos::of_input(&live, shared.config.repo.as_deref());
    let folders = checkouts.into_iter().filter_map(|checkout| checkout.directory);
    folders.fold(Known::default(), |known, folder| known.joined(&agent_engine::tools::env_secrets(&folder)))
}

pub(crate) async fn poll(gated: &Gated<'_>, who: Who<'_>, command: &Command, key: &str) -> Result<Value, Failure> {
    let every = Duration::from_millis(command.args["every"].as_u64().unwrap_or(30_000).max(10));
    let until = &command.args["until"];
    loop {
        let wait = match labelled(gated.shared, who, &command.call(), key).await {
            Ok(read) if matches(until, &read.value) => return gated.read(read),
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
