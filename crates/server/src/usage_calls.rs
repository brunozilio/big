use std::sync::Arc;

use connectors::calls::{CallSink, Called};
use domain::spend::{Charged, SpendSink};
use store::{ConnectorCall, Spent, Store};

#[derive(Debug)]
pub struct StoredCalls(pub Arc<Store>);

impl CallSink for StoredCalls {
    fn called(&self, call: &Called<'_>) {
        let attribution = call.attribution;
        let counted = ConnectorCall {
            at: crate::marks::now_ms(),
            connection: call.connection.as_str(),
            action: call.action,
            origin: attribution.map(|charged| charged.origin.word()),
            item: attribution.and_then(|charged| charged.item.as_deref()),
            step: attribution.and_then(|charged| charged.step.as_deref()),
            ok: call.ok,
        };
        if let Err(error) = self.0.count_connector_call(&counted) {
            tracing::warn!(%error, connection = %call.connection, "a connector call could not be counted");
        }
    }
}

#[derive(Debug)]
pub struct StoredSpend(pub Arc<Store>);

impl SpendSink for StoredSpend {
    fn spent(&self, charged: &Charged<'_>) {
        let attribution = charged.attribution;
        let spent = Spent {
            at: crate::marks::now_ms(),
            item: attribution.item.clone().unwrap_or_default(),
            step: attribution.step.clone().unwrap_or_default(),
            agent: charged.agent.to_owned(),
            application: String::new(),
            owner: charged.owner.to_owned(),
            origin: attribution.origin,
            cost: charged.cost,
        };
        if let Err(error) = self.0.record_spend(&spent) {
            tracing::warn!(%error, owner = charged.owner, "a conversation's spend could not be kept");
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::billing::{Basis, Cost};
    use domain::origin::{Attribution, Origin};
    use domain::quota::Scope;
    use store::ConnectorCalls;

    use super::*;

    fn row(connection: &str, origin: Option<&str>, attributed: bool, calls: u64, failed: u64) -> ConnectorCalls {
        ConnectorCalls { connection: connection.into(), origin: origin.map(str::to_owned), attributed, calls, failed }
    }

    #[test]
    fn a_counted_call_lands_in_the_store_with_its_origin_and_item() {
        let store = Arc::new(Store::in_memory(0).expect("store"));
        let sink = StoredCalls(Arc::clone(&store));
        let charged = Attribution::of_step("i1", "develop", Origin::Step);
        let (connection, provider) =
            (domain::connection::ConnectionId::from("c1"), domain::connection::ProviderKey::from("p"));
        let called =
            |attribution, ok| Called { connection: &connection, provider: &provider, action: "a.b", attribution, ok };
        sink.called(&called(Some(&charged), true));
        sink.called(&called(None, false));
        let rows = store.connector_calls_since(0).expect("rows");
        assert_eq!(rows, [row("c1", None, false, 1, 1), row("c1", Some("step"), true, 1, 0)]);
    }

    #[test]
    fn a_conversation_s_spend_counts_for_its_person_ceiling() {
        let store = Arc::new(Store::in_memory(0).expect("store"));
        let sink = StoredSpend(Arc::clone(&store));
        let chat = Attribution::default();
        let title = Attribution { origin: Origin::Title, ..Attribution::default() };
        for attribution in [&chat, &title] {
            let cost = Cost::priced(Some(0.5), Basis::Api);
            sink.spent(&Charged { owner: "u-ana", agent: "", attribution, cost });
        }
        let kept = store.spent_since(0, Scope::Person, "u-ana").expect("spend");
        assert_eq!(kept.known_micros, 1_000_000);
    }
}
