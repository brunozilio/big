mod delivery;
mod sink;

use std::sync::Arc;

use connectors::Connectors;
use domain::connection::ProviderKey;
use store::Store;

pub use delivery::{IDLE, OFFER, Outbox, Step, backoff, event_of};
pub use sink::{ConnectorSink, Sink};

pub fn start(store: &Arc<Store>, connectors: &Arc<Connectors>, sinks: &[String]) {
    for name in sinks {
        let key = ProviderKey::from(name.as_str());
        if connectors.descriptor(&key).is_err() {
            tracing::warn!(sink = name, "no connector is called like the audit sink; its events wait in the trail");
        }
        let sink = ConnectorSink { key, store: Arc::clone(store), connectors: Arc::clone(connectors) };
        let outbox = Outbox::new(Arc::clone(store), Arc::new(sink));
        tokio::spawn(outbox.run());
        tracing::info!(sink = name, "audit events are delivered to the sink");
    }
}

#[cfg(test)]
mod tests;
