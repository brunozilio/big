use std::sync::Arc;
use std::time::Duration;

use connectors::Connectors;
use connectors::contracts::{MCP, mcp};
use domain::mcp::RemoteMcp;
use store::{ConnectionRecord, Store};

use crate::calls::connection;

const WAITS: Duration = Duration::from_secs(20);

fn offers_mcp(record: &ConnectionRecord) -> Option<&str> {
    let url = record.settings[mcp::URL].as_str().filter(|url| !url.is_empty())?;
    record.capabilities.iter().any(|capability| capability == MCP).then_some(url)
}

pub async fn servers(store: &Store, connectors: &Connectors) -> Vec<RemoteMcp> {
    let records = store.connections().unwrap_or_default();
    let mut found = Vec::new();
    for record in &records {
        let Some(url) = offers_mcp(record) else { continue };
        match connectors.access_token(&connection(record)).await {
            Ok(token) => found.extend(RemoteMcp::named(&record.label, url, &token)),
            Err(error) => {
                tracing::warn!(connection = %record.id, message = %error.message, "a connected MCP server is left out of the run");
            }
        }
    }
    found
}

pub fn for_runs(store: Arc<Store>, connectors: Arc<Connectors>, runtime: tokio::runtime::Handle) -> app::ConnectedMcp {
    app::ConnectedMcp::new(move || {
        let found = async { tokio::time::timeout(WAITS, servers(&store, &connectors)).await.unwrap_or_default() };
        std::thread::scope(|scope| scope.spawn(|| runtime.block_on(found)).join().unwrap_or_default())
    })
}

#[cfg(test)]
mod tests {
    use connectors::{FakeHttp, Grant, MemoryTokenStore, SystemClock, TokenKeeper};
    use serde_json::json;

    use super::*;

    fn record(id: &str, label: &str, capabilities: &[&str], settings: serde_json::Value) -> ConnectionRecord {
        ConnectionRecord {
            id: id.to_owned(),
            provider: if capabilities.contains(&MCP) { "mcp" } else { "jira" }.to_owned(),
            label: label.to_owned(),
            account: Some(label.to_owned()),
            capabilities: capabilities.iter().map(|name| (*name).to_owned()).collect(),
            settings,
        }
    }

    async fn kept() -> (Arc<Store>, Arc<Connectors>) {
        let store = Arc::new(Store::in_memory(0).expect("store"));
        let tokens = Arc::new(MemoryTokenStore::new());
        let http = Arc::new(FakeHttp::new());
        let keeper = TokenKeeper::new(tokens, http.clone(), Arc::new(SystemClock));
        let hub = Arc::new(Connectors::new(http, keeper));
        let saved = [
            (record("m1", "Linear Prod", &[MCP], json!({ "url": "https://linear.test/mcp" })), Some("at-1")),
            (record("m2", "Aberto", &[MCP], json!({ "url": "http://localhost:9/mcp" })), Some("")),
            (record("m3", "Sem token", &[MCP], json!({ "url": "https://gone.test/mcp" })), None),
            (record("j1", "acme", &["tracker"], json!({ "url": "https://acme.test" })), Some("jt")),
        ];
        for (record, token) in saved {
            store.save_connection(&record).expect("saved");
            if let Some(token) = token {
                let id = domain::connection::ConnectionId::from(record.id.as_str());
                hub.keeper().keep(&id, &Grant::bearer(token)).await.expect("kept");
            }
        }
        (store, hub)
    }

    #[tokio::test]
    async fn every_connected_mcp_server_goes_to_runs_with_its_current_token() {
        let (store, hub) = kept().await;
        let found = servers(&store, &hub).await;
        let shown: Vec<(String, String, Option<String>)> =
            found.into_iter().map(|remote| (remote.name, remote.url, remote.token)).collect();
        assert_eq!(
            shown,
            [
                ("linear-prod".to_owned(), "https://linear.test/mcp".to_owned(), Some("at-1".to_owned())),
                ("aberto".to_owned(), "http://localhost:9/mcp".to_owned(), None),
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn runs_ask_for_them_from_any_thread() {
        let (store, hub) = kept().await;
        let hook = for_runs(store, hub, tokio::runtime::Handle::current());
        let names = tokio::task::spawn_blocking(move || hook.servers().len()).await.expect("joined");
        assert_eq!(names, 2);
    }
}
