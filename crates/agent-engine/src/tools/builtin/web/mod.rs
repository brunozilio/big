pub mod guarded;
pub mod reach;

use crate::model::{CancellationToken, ModelError};
use crate::tools::field;

pub fn read_by_a_reader(ran: &crate::tools::after::Ran<'_>, page: &str) -> String {
    let (url, looking_for) = (field(ran.input, "url"), field(ran.input, "prompt"));
    let (models, agent) = (&ran.workspace.models, ran.agent);
    let reader = guarded::reader(
        models.small_model.as_deref(),
        agent.request,
        models.model.provider,
        domain::origin::Origin::Filter,
    );
    let read = if ran.workspace.policy.web.small_pages_direct { guarded::digest } else { guarded::read_apart };
    read(
        agent.api,
        &reader,
        &agent.request.model,
        url.unwrap_or_default(),
        looking_for.unwrap_or_default(),
        page,
        agent.stop,
    )
}

pub fn under<T: Send>(
    cancel: &CancellationToken,
    work: impl std::future::Future<Output = Result<T, String>> + Send,
) -> Result<T, String> {
    let raced = async {
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(ModelError::Cancelled.to_string()),
            done = work => done,
        }
    };
    platform::runtime::block_on(raced).unwrap_or_else(|error| Err(error.to_string()))
}
