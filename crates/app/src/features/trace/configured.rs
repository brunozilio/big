use std::sync::Arc;

use agent_engine::sessions::FileSessions;
use domain::redact::Redactor;
use domain::trace::capture::{Keys, Sharing};

use super::export::{Content, Exporter, Otlp, Policy, Secrets, Traced};

pub(crate) fn exporter(sessions: &Arc<FileSessions>, secrets: &Redactor) -> Option<Exporter> {
    if Sharing::read(brand::traces::SHARING) == Sharing::Disabled {
        return None;
    }
    let otlp = Otlp::from(brand::traces::OTLP_ENDPOINT, brand::traces::OTLP_HEADERS)?;
    let store = Arc::clone(sessions);
    let traced: Traced = Arc::new(move |cwd, id| super::traced(&store, cwd, id));
    let exporter = Exporter::start(otlp, traced, Arc::new(agent_engine::model::http::NetHttp))?;
    exporter.capturing(policy(secrets));
    Some(exporter)
}

pub(crate) fn policy(secrets: &Redactor) -> Policy {
    let sharing = Sharing::read(brand::traces::SHARING);
    let configured = Keys::configured();
    let keys = configured.shared(sharing);
    let redactor = agent_engine::tools::personal(secrets);
    let revision = domain::trace::capture::revision(configured, sharing, redactor.rules());
    let folder_secrets: Secrets = Arc::new(agent_engine::tools::env_secrets);
    let content = Content::from(keys, brand::traces::CAPTURE_LIMIT, || redactor, folder_secrets);
    Policy { content, usage: keys.usage, revision }
}
