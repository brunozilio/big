mod backoff;
mod delivery;
mod event;
mod sink;

use std::sync::Arc;

use connectors::Connectors;
use connectors::audit_sink::DELIVER;
use domain::connection::ProviderKey;
use store::Store;

pub use backoff::Pace;
pub use delivery::{Outbox, Step};
pub use event::event_of;
pub use sink::{ConnectorSink, Sink};

pub fn checked(connectors: &Connectors, sinks: &[String]) -> Result<(), String> {
    for name in sinks {
        let key = ProviderKey::from(name.as_str());
        let descriptor = connectors.descriptor(&key).map_err(|_| {
            format!("[audit] sinks names `{name}`, but no connector in content/connectors is called like that")
        })?;
        if descriptor.action(DELIVER).is_none() {
            return Err(format!("[audit] sinks names `{name}`, a connector that offers no {DELIVER}"));
        }
    }
    Ok(())
}

pub fn start(store: &Arc<Store>, connectors: &Arc<Connectors>, sinks: &[String]) -> Result<(), String> {
    if sinks.is_empty() {
        return Ok(());
    }
    checked(connectors, sinks)?;
    let pace = Pace::configured()?;
    for name in sinks {
        let key = ProviderKey::from(name.as_str());
        let sink = ConnectorSink { key, store: Arc::clone(store), connectors: Arc::clone(connectors) };
        tokio::spawn(Outbox::new(Arc::clone(store), Arc::new(sink), pace).run());
        tracing::info!(sink = name, "audit events are delivered to the sink");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
