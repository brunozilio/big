use std::sync::Arc;

use domain::effect::{EffectOutcome, IdempotencyKey};
use domain::workflow::{Ending, ItemId, WorkflowEvent};

use crate::error::Result;
use crate::ledger::Ledger;
use crate::protocol::{CHILD, Command, kind_of};
use crate::shared::Shared;

pub fn child_id(parent: &ItemId, key: &IdempotencyKey) -> ItemId {
    ItemId::from(format!("{parent}-{}-{}", key.seq, key.attempt))
}

pub fn start(shared: &Arc<Shared>, parent: &ItemId, command: &Command, key: &IdempotencyKey) -> Result<()> {
    let id = child_id(parent, key);
    let workflow = command.text("workflow");
    if shared.store.item(&id)?.is_none() {
        let created = WorkflowEvent::ItemCreated {
            id: id.clone(),
            workflow: workflow.clone(),
            parent: Some(parent.clone()),
            input: command.args["input"].clone(),
        };
        shared.append(&id, &[created])?;
    }
    if !shared.item(parent)?.children.iter().any(|link| link.id == id) {
        shared.append(parent, &[WorkflowEvent::ChildStarted { child: id.clone(), workflow }])?;
    }
    crate::runner::ensure(shared, &id);
    Ok(())
}

pub fn sweep(shared: &Shared, parent: &ItemId, ledger: &Ledger) -> Result<()> {
    let waiting: Vec<IdempotencyKey> =
        ledger.open().filter(|request| kind_of(&request.effect) == CHILD).map(|request| request.key.clone()).collect();
    for key in waiting {
        let child = child_id(parent, &key);
        let Some(Some(ending)) = shared.store.item(&child)?.map(|state| state.ending) else { continue };
        let (output, outcome) = match ending {
            Ending::Finished { output } => (output.clone(), EffectOutcome::Ok { value: output }),
            Ending::Cancelled { reason } => (serde_json::json!({ "cancelled": reason }), EffectOutcome::Cancelled),
        };
        let linked = shared.item(parent)?.children.iter().any(|link| link.id == child && !link.finished);
        if linked {
            shared.append(parent, &[WorkflowEvent::ChildFinished { child, output }])?;
        }
        shared.resolve(parent, &key, outcome)?;
    }
    Ok(())
}
