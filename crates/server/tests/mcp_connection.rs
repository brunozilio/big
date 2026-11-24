#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use support::{World, fixture};

async fn connected(world: &World, fields: Value) -> (StatusCode, Value) {
    let (status, started) =
        world.send(Method::POST, "/api/connections/mcp/start", Some(json!({ "fields": fields }))).await;
    if status != StatusCode::OK {
        return (status, started);
    }
    assert_eq!(started["kind"], "token", "{started}");
    let flow = json!({ "flow": started["flow"] });
    (status, world.send(Method::POST, "/api/connections/device/poll", Some(flow)).await.1)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_remote_mcp_server_becomes_a_connection_whose_tools_every_run_gets() {
    let world = World::new().await;
    let listed = world.get("/api/connections").await;
    let provider = listed["providers"].as_array().and_then(|all| all.iter().find(|p| p["key"] == "mcp")).cloned();
    let provider = provider.expect("the MCP provider is listed");
    assert_eq!((provider["signIn"].as_str(), provider["group"].as_str()), (Some("oauth"), Some("Ferramentas")));
    let names: Vec<&str> =
        provider["fields"].as_array().into_iter().flatten().filter_map(|f| f["name"].as_str()).collect();
    assert_eq!(names, ["url", "label"]);
    let unsafe_address = connected(&world, json!({ "url": "http://open.test/mcp", "label": "Docs" })).await;
    assert_eq!(unsafe_address.0, StatusCode::BAD_REQUEST, "{}", unsafe_address.1);
    let (status, done) = connected(&world, json!({ "url": " https://open.test/mcp ", "label": "Docs Abertos" })).await;
    assert_eq!((status, done["state"].as_str()), (StatusCode::OK, Some("done")), "{done}");
    assert_eq!(done["connection"]["label"], "Docs Abertos");
    assert_eq!(done["connection"]["capabilities"], json!(["mcp"]));
    let id = done["connection"]["id"].as_str().expect("id");
    let record = world.store.connection(id).expect("store").expect("record");
    assert_eq!(record.settings["url"], "https://open.test/mcp");
    let call = json!({ "action": "mcp.tools", "args": {} });
    let (status, tools) = world.send(Method::POST, &format!("/api/connections/{id}/call"), Some(call)).await;
    assert_eq!((status, tools.clone()), (StatusCode::OK, json!([{ "name": "search" }])));
    fixture("call-mcp-tools", &tools);
    let body = json!({ "text": "procure nos docs", "issue": null, "model": "m1", "attachments": [] });
    let (status, item) = world.send(Method::POST, "/api/items", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    world.until("the agent ran", |item| item["group"] == "you").await;
    let remote: Vec<domain::mcp::RemoteMcp> =
        world.engine.requests().into_iter().flat_map(|request| request.host.credentials.remote_mcp).collect();
    let expected = domain::mcp::RemoteMcp::named("Docs Abertos", "https://open.test/mcp", "");
    assert!(!remote.is_empty() && remote.iter().all(|found| Some(found) == expected.as_ref()), "{remote:?}");
}
