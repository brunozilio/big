mod support;

use axum::http::{Method, StatusCode};
use support::World;

#[tokio::test(flavor = "multi_thread")]
async fn cloud_credentials_for_a_token_no_agent_holds_are_not_found() {
    let world = World::new().await;
    let (status, body) = world.send(Method::GET, "/api/cloud/credentials/nope", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");
}
