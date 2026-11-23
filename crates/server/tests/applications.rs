#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use support::{World, fixture};

fn folder(world: &World, name: &str) -> String {
    let path = world.dir.path().join(name);
    std::fs::create_dir_all(&path).expect("dir");
    std::fs::canonicalize(path).expect("path").to_string_lossy().into_owned()
}

fn steady(listed: &Value) -> Value {
    let mut kept = listed.clone();
    for (at, application) in kept.as_array_mut().into_iter().flatten().enumerate() {
        application["id"] = json!(format!("app-{at}"));
        for (place, repository) in application["repositories"].as_array_mut().into_iter().flatten().enumerate() {
            repository["id"] = json!(format!("repo-{at}-{place}"));
            let name =
                repository["directory"].as_str().and_then(|dir| dir.rsplit('/').next()).unwrap_or_default().to_owned();
            repository["directory"] = json!(format!("/projetos/{name}"));
        }
    }
    kept
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_run_starts_without_any_application() {
    let world = World::new().await;
    let launch = world.dir.path().join("repo");
    let done = platform::git::command(&launch).args(["init", "-q"]).output();
    assert!(done.is_ok_and(|done| done.status.success()));
    assert_eq!(world.get("/api/applications").await, json!([]), "the onboarding asks for the first one");
}

#[tokio::test(flavor = "multi_thread")]
async fn applications_are_created_changed_checked_and_removed() {
    let world = World::new().await;
    let (web, api) = (folder(&world, "web"), folder(&world, "api"));
    let body = json!({
        "name": " Loja ", "tracker": { "connection": "c1", "project": "APP" },
        "repositories": [
            { "code": { "connection": "c1", "repo": "acme/web" }, "directory": web, "run": "bun dev",
              "verify": ["bun test", " "], "e2e": [" npx playwright test ", ""] },
            { "code": null, "directory": api, "run": "", "verify": [] },
        ],
    });
    let (status, made) = world.send(Method::POST, "/api/applications", Some(body.clone())).await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["name"], "Loja");
    assert_eq!(made["repositories"][0]["verify"], json!(["bun test"]));
    assert_eq!(made["repositories"][0]["e2e"], json!(["npx playwright test"]));
    assert_eq!(made["repositories"][1]["e2e"], json!([]));
    let ids: Vec<&str> =
        made["repositories"].as_array().expect("repos").iter().filter_map(|repo| repo["id"].as_str()).collect();
    assert!(ids.len() == 2 && ids[0] != ids[1] && !ids[0].is_empty());
    let id = made["id"].as_str().expect("id").to_owned();

    let mut changed = made.clone();
    changed["name"] = json!("Loja nova");
    let (status, saved) = world.send(Method::PUT, &format!("/api/applications/{id}"), Some(changed)).await;
    assert_eq!((status, saved["name"].as_str()), (StatusCode::OK, Some("Loja nova")));
    assert_eq!(saved["repositories"][0]["id"], made["repositories"][0]["id"]);
    let listed = world.get("/api/applications").await;
    assert_eq!(listed.as_array().map(Vec::len), Some(1), "only what was made, nothing seeded");
    fixture("applications", &steady(&listed));

    let nameless = json!({ "name": " ", "repositories": body["repositories"] });
    assert_eq!(world.send(Method::POST, "/api/applications", Some(nameless)).await.0, StatusCode::BAD_REQUEST);
    let lost = json!({ "name": "x", "repositories": [{ "directory": "/não/existe" }] });
    let (status, error) = world.send(Method::POST, "/api/applications", Some(lost)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error["error"]["message"].as_str().is_some_and(|text| text.contains("não existe")));
    assert_eq!(world.send(Method::PUT, "/api/applications/zz", Some(made.clone())).await.0, StatusCode::NOT_FOUND);
    assert_eq!(world.send(Method::DELETE, &format!("/api/applications/{id}"), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(world.send(Method::DELETE, &format!("/api/applications/{id}"), None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_item_keeps_a_snapshot_of_its_application_and_its_space() {
    let world = World::new().await;
    let web = folder(&world, "web");
    let body = json!({
        "name": "Loja", "tracker": { "connection": "c1", "project": "APP" },
        "repositories": [{ "code": { "connection": "c1", "repo": "acme/web" }, "directory": web, "verify": ["true"] }],
    });
    let made = world.send(Method::POST, "/api/applications", Some(body)).await.1;
    let item =
        json!({ "text": "Exportar", "issue": null, "model": "auto", "attachments": [], "application": made["id"] });
    let (status, created) = world.send(Method::POST, "/api/items", Some(item)).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id = domain::workflow::ItemId::from(created["id"].as_str().expect("id"));
    let kept = world.handle.item(&id).expect("store").expect("item");
    assert_eq!((kept.input["project"].as_str(), kept.input["connection"].as_str()), (Some("APP"), Some("c1")));
    assert_eq!(kept.input["application"]["name"], "Loja");
    assert_eq!(kept.input["application"]["repositories"][0]["code"]["pipelines"], true);
    assert_eq!(kept.input["application"]["repositories"][0]["directory"], json!(web));

    let gone = json!({ "text": "x", "issue": null, "model": "auto", "attachments": [], "application": "zz" });
    assert_eq!(world.send(Method::POST, "/api/items", Some(gone)).await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_folder_browser_lists_subfolders_and_tells_the_repositories() {
    let world = World::new().await;
    let root = folder(&world, "pastas");
    std::fs::create_dir_all(format!("{root}/b-projeto/.git")).expect("dir");
    std::fs::create_dir_all(format!("{root}/a-docs")).expect("dir");
    std::fs::create_dir_all(format!("{root}/.oculta")).expect("dir");
    std::fs::write(format!("{root}/arquivo.txt"), "x").expect("file");
    let listed = world.get(&format!("/api/fs/dirs?path={root}")).await;
    assert_eq!(listed["path"], json!(root));
    assert_eq!(listed["repo"], false);
    let names: Vec<(&str, bool)> = listed["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| (entry["name"].as_str().unwrap_or_default(), entry["repo"].as_bool().unwrap_or_default()))
        .collect();
    assert_eq!(names, [("a-docs", false), ("b-projeto", true)]);
    assert!(listed["parent"].as_str().is_some_and(|parent| root.starts_with(parent)));
    let home = world.get("/api/fs/dirs").await;
    assert_eq!(home["path"], home["home"]);
    assert_eq!(world.send(Method::GET, "/api/fs/dirs?path=relativo", None).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(world.send(Method::GET, &format!("/api/fs/dirs?path={root}/nada"), None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_page_lists_the_repositories_of_a_code_connection() {
    let world = World::new().await;
    let body = json!({ "action": "code.repos", "args": { "query": "web" } });
    let (status, listed) = world.send(Method::POST, "/api/connections/c1/call", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        listed,
        json!([{ "id": "10", "ref": "acme/web", "name": "web", "url": "https://code.test/acme/web",
                 "defaultBranch": "main", "private": true }])
    );
}
