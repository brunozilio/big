use connectors::Connectors;
use connectors::contracts::{self, tracker};
use domain::effect::{Effect, EffectRequest, IdempotencyKey};
use domain::workflow::WorkItem;
use serde_json::{Value, json};
use store::Store;

use crate::book::Book;
use crate::desired::desired_of;
use crate::error::Error;
use crate::policy::ReconcilePolicy;

#[derive(Clone, Debug, PartialEq)]
pub enum Pushed {
    NotSynced,
    NoIssue,
    NoConnection,
    Done(Value),
}

pub async fn push(
    connectors: &Connectors,
    store: &Store,
    item: &WorkItem,
    request: &EffectRequest,
) -> Result<Pushed, Error> {
    match &request.effect {
        Effect::StatusSet { name } => push_status(connectors, store, item, name, &request.key).await,
        Effect::ImpedimentSet { on, reason } => {
            push_impediment(connectors, store, item, *on, reason.as_deref(), &request.key).await
        }
        _ => Ok(Pushed::NotSynced),
    }
}

pub async fn push_status(
    connectors: &Connectors,
    store: &Store,
    item: &WorkItem,
    status: &str,
    key: &IdempotencyKey,
) -> Result<Pushed, Error> {
    send(connectors, store, item, tracker::TRANSITION, json!({ "status": status }), key).await
}

pub async fn push_impediment(
    connectors: &Connectors,
    store: &Store,
    item: &WorkItem,
    on: bool,
    reason: Option<&str>,
    key: &IdempotencyKey,
) -> Result<Pushed, Error> {
    let args = match reason {
        Some(reason) => json!({ "on": on, "reason": reason }),
        None => json!({ "on": on }),
    };
    send(connectors, store, item, tracker::SET_IMPEDIMENT, args, key).await
}

async fn send(
    connectors: &Connectors,
    store: &Store,
    item: &WorkItem,
    action: &str,
    mut args: Value,
    key: &IdempotencyKey,
) -> Result<Pushed, Error> {
    let Some(issue) = desired_of(item, ReconcilePolicy::default()).issue else { return Ok(Pushed::NoIssue) };
    let book = Book::load(store)?;
    let owner = domain::principal::owner_in(&item.input);
    let Some(entry) = book.resolve_for(owner, issue.connection.as_deref(), contracts::TRACKER) else {
        return Ok(Pushed::NoConnection);
    };
    args["issue"] = Value::String(issue.issue);
    let charged = domain::origin::Attribution::of_step(key.item.clone(), key.step.clone(), domain::origin::Origin::Step);
    let value =
        connectors.call_for(Some(&charged), &entry.connection, &entry.settings, action, &args, &key.to_string()).await?;
    Ok(Pushed::Done(value))
}
