use axum::extract::{Path, State};
use axum::response::Response;
use domain::board::Group;
use domain::workflow::ItemId;

use crate::error::{ApiError, ApiResult, Spoken, done, said};
use crate::marks::now_ms;
use crate::state::{Ctx, Shared};

fn unknown() -> Spoken {
    said("There is no such item.", "No existe ese ítem.", "Não há esse item.")
}

fn delivered(shared: &Shared, id: &ItemId) -> ApiResult<()> {
    let summary = shared.backend.orchestrator.summary(id)?.ok_or_else(|| ApiError::missing(unknown()))?;
    if summary.group == Group::Delivered {
        return Ok(());
    }
    Err(ApiError::bad(said(
        "Only delivered items can be archived.",
        "Solo se puede archivar lo que ya se entregó.",
        "Só dá para arquivar o que já foi entregue.",
    )))
}

fn told(shared: &Shared, id: &ItemId) -> ApiResult<Response> {
    if let Some(item) = crate::items::changed(shared, id)? {
        shared.live.item(item);
    }
    Ok(done())
}

pub async fn archive(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let id = ItemId::from(id.as_str());
    delivered(&ctx, &id)?;
    ctx.backend.store.archive_item(id.as_str(), now_ms())?;
    told(&ctx, &id)
}

pub async fn unarchive(State(ctx): State<Ctx>, Path(id): Path<String>) -> ApiResult<Response> {
    let id = ItemId::from(id.as_str());
    if ctx.backend.orchestrator.summary(&id)?.is_none() {
        return Err(ApiError::missing(unknown()));
    }
    ctx.backend.store.unarchive_item(id.as_str())?;
    told(&ctx, &id)
}
