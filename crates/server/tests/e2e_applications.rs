#![allow(clippy::expect_used, clippy::panic, reason = "a failed step stops the test")]

mod e2e;

use e2e::{CODE, Hub, step, waiting_on};
use serde_json::json;

fn second_repo(hub: &Hub) -> std::path::PathBuf {
    let dir = hub.dir.path().join("api");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("API.md"), "api\n").expect("file");
    for args in [vec!["init", "-q", "-b", "main"], vec!["add", "-A"], vec!["commit", "-qm", "início"], vec!["branch", "develop"]] {
        let done = platform::git::command(&dir).args(["-c", "user.email=a@b", "-c", "user.name=a"]).args(args).output();
        assert!(done.is_ok_and(|done| done.status.success()));
    }
    dir
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_of_an_application_with_two_repositories_delivers_a_pull_request_in_each() {
    let hub = Hub::new();
    let api = second_repo(&hub);
    hub.github.add("acme/api", api.clone());
    let server = hub.start().await;
    let application = json!({
        "name": "Loja", "bindings": { "tracker": { "connection": "trk", "value": "K" }, "tests": { "connection": "trk", "value": "K" } },
        "repositories": [
            { "directory": hub.repo(), "bindings": { "code": { "connection": CODE, "value": "acme/app" } }, "verify": ["test -f README.md"] },
            { "directory": api, "bindings": { "code": { "connection": CODE, "value": "acme/api" } }, "verify": ["test -f API.md"] },
        ],
    });
    let made = server.post("/api/applications", application).await;
    let body = json!({ "text": "Mudar as duas pontas", "issue": null, "model": "auto", "attachments": [], "application": made["id"] });
    let id = server.create(body).await;
    server.until(&id, "the item names its application", |item| item["application"]["name"] == "Loja" && item["application"]["id"] == made["id"]).await;
    server.approve_plan(&id).await;

    for _ in 0..2 {
        server.until(&id, "a review waits", |item| waiting_on(item, "review")).await;
        server.input(&id, "review", "review", json!({ "verdict": "approve", "body": "ok", "notes": [] })).await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    server.until(&id, "the merge waits", |item| waiting_on(item, "merge")).await;
    let merge = server.get(&format!("/api/items/{id}/steps/merge")).await;
    let merges = merge["blocks"].as_array().expect("blocks").iter().filter(|block| block["type"] == "merge").count();
    assert_eq!(merges, 2, "{merge}");
    server.input(&id, "merge", "merge", json!({ "method": "squash" })).await;
    for _ in 0..2 {
        server.until(&id, "a deploy waits", |item| waiting_on(item, "release")).await;
        server.input(&id, "release", "approve_deploy", json!({})).await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let done = server.until(&id, "the delivery", |item| item["group"] == "delivered").await;
    assert_eq!(done["status"], "Feito");
    assert_eq!((hub.github.pulls_of("acme/app"), hub.github.pulls_of("acme/api")), (2, 2));
    let routes: Vec<(String, String)> = hub.github.state().pulls.iter().map(|pull| (pull.head.clone(), pull.base.clone())).collect();
    let promoted = ("develop".to_owned(), "release".to_owned());
    assert_eq!(routes.iter().filter(|route| **route == promoted).count(), 2, "the flow promotes develop into release: {routes:?}");
    assert_eq!(hub.github.state().merges, 4, "each repository merges its pull request and its promotion");
    assert_eq!(hub.github.state().deploy_reviews, ["approved", "approved"]);
    let checks = server.get(&format!("/api/items/{id}/steps/checks")).await;
    let logs = checks["blocks"][0]["commands"].to_string();
    assert!(logs.contains("README.md") && logs.contains("API.md"), "{checks}");
    assert_eq!(step(&done, "ci")["state"], "passed");
    let ci = server.get(&format!("/api/items/{id}/steps/ci")).await;
    assert_eq!(ci["blocks"].as_array().map(Vec::len), Some(2), "{ci}");
}
