use axum::Router;
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use connectors::Connectors;
use domain::Language;
use serde_json::{Value, json};

use crate::error::json as reply;
use crate::state::Ctx;

pub fn routes(router: Router<Ctx>) -> Router<Ctx> {
    router.route("/api/events", get(list))
}

pub fn listing(connectors: &Connectors, language: Language) -> Value {
    json!({ "connectors": connectors.catalog(language), "platform": orchestrator::platform_events() })
}

pub async fn list(State(ctx): State<Ctx>) -> Response {
    reply(listing(&ctx.backend.connectors, ctx.language()))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use connectors::scripted::ScriptedProvider;
    use connectors::{FakeHttp, FixedClock, MemoryTokenStore, Provider, TokenKeeper};

    use super::*;

    const SOURCE: &str = r#"export function provider() {
  return { schema_version: 2, key: "acme", name: "Acme", capabilities: ["tracker"], auth: { kind: "token", fields: [], form: "bearer" }, actions: [] };
}
export function events({ language }) {
  return [{ name: "acme.ticket.opened", label: language === "en" ? "Ticket opened" : "Ticket aberto", capability: "tracker", source: "issue",
    aliases: ["tracker.issue.created"], schema: { type: "object", properties: { title: { type: "string" } } }, filters: ["title"] }];
}
export function poll() { return { events: [], cursor: "" }; }
export function call() { return {}; }
export function accounts() { return []; }
"#;

    #[test]
    fn the_listing_names_each_connector_event_and_the_platform_ones() {
        let root = tempfile::tempdir().expect("dir");
        let folder = root.path().join("connectors/acme");
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(folder.join("index.ts"), SOURCE).expect("source");
        let provider = ScriptedProvider::load_in(Some(root.path()), "connectors/acme/index.ts").expect("loads");
        let providers: Vec<Arc<dyn Provider>> = vec![Arc::new(provider)];
        let keeper = TokenKeeper::new(
            Arc::new(MemoryTokenStore::new()),
            Arc::new(FakeHttp::new()),
            Arc::new(FixedClock::at_second(1)),
        );
        let hub = Connectors::from_providers(providers, Arc::new(FakeHttp::new()), keeper);
        let listed = listing(&hub, Language::English);
        let event = &listed["connectors"][0]["events"][0];
        assert_eq!(listed["connectors"][0]["connector"], "acme");
        assert_eq!(event["name"], "acme.ticket.opened");
        assert_eq!(event["label"], "Ticket opened");
        assert_eq!(event["filters"], json!(["title"]));
        assert_eq!(event["aliases"], json!(["tracker.issue.created"]));
        assert_eq!((event["cadence"].as_str(), event["backfill"].as_str()), (Some("poll"), Some("none")));
        let platform = listed["platform"].as_array().expect("platform");
        assert!(platform.iter().any(|event| event["name"] == "item.step.finished"));
    }
}
