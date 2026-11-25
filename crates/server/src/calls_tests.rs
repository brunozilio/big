use super::*;

#[test]
fn an_issue_takes_the_page_s_shape() {
    let raw = json!({
        "ref": "K-1", "url": "https://t/K-1", "title": "T", "status": "A fazer", "description": "d",
        "subtasks": [{ "ref": "K-2", "title": "s", "status": "Pronto", "done": true }],
        "attachments": [{ "name": "a.png", "size": 10, "url": "https://t/a" }],
        "comments": [{ "id": "1", "author": { "id": "u1", "name": "Ana" }, "body": "oi", "at": "2024-01-01T00:00:00Z" }],
    });
    let shown = issue(&raw);
    assert_eq!(shown["key"], "K-1");
    assert_eq!(shown["subtasks"][0]["done"], true);
    assert_eq!(shown["attachments"][0]["size"], "10");
    assert_eq!(shown["comments"][0]["author"]["name"], "Ana");
    assert_eq!(shown["comments"][0]["author"]["id"], "u1");
    assert_eq!(shown["assignee"], Value::Null);
}

#[test]
fn a_change_and_a_pipeline_take_the_page_s_shape() {
    let pull = json!({ "ref": "o/r#3", "number": 3, "title": "PR", "url": "https://c/3", "state": "open", "head": "h", "base": "main",
        "author": "bia", "reviews": [{ "id": "1", "author": "bia", "verdict": "approve", "body": "", "at": "x" }] });
    let files = json!([{ "path": "a.rs", "change": "modified", "additions": 1, "deletions": 0, "patch": "@@ -1,2 +4,3 @@\n+x" }]);
    let threads = json!([{ "id": "c", "thread": "t", "path": "a.rs", "line": 4, "author": "bia", "body": "?", "at": "x", "resolved": false }]);
    let data = change_data("o/r#3", &pull, &files, &threads);
    assert_eq!(data["files"][0]["firstLine"], 4);
    assert_eq!(data["comments"][0]["kind"], "approved");
    assert_eq!(data["comments"][1]["line"], 4);
    assert_eq!(data["author"]["name"], "bia");
    let by_agent =
        json!([{ "id": "d", "path": "a.rs", "line": 4, "author": "bia", "agent": "reviewer", "body": "!", "at": "x" }]);
    let author = &change_data("o/r#3", &pull, &files, &by_agent)["comments"][1]["author"];
    assert_eq!((author["name"].as_str(), author["agent"].as_bool()), (Some("reviewer"), Some(true)));
    let run = json!({ "ref": "o/r/runs/9", "pipeline": "CI", "state": "failed", "head": "h", "jobs": [
        { "id": "1", "key": "build", "name": "build", "state": "passed", "needs": [], "steps": [] },
        { "id": "2", "key": "test", "name": "test", "state": "cancelled", "needs": ["build"],
          "steps": [{ "name": "x", "state": "running", "started": null, "finished": null }] },
    ] });
    let shown = pipeline_run("o/r/runs/9", &run);
    assert_eq!(shown["state"], "failed");
    assert_eq!(shown["ref"], "h");
    assert_eq!(shown["jobs"][1]["stage"], 1);
    assert_eq!(shown["jobs"][1]["state"], "failed");
    assert_eq!(shown["jobs"][1]["steps"][0]["state"], "running");
}

#[test]
fn the_cloud_accounts_take_the_page_s_shape() {
    let raw = json!({ "accounts": [
        { "id": "111", "name": "prod", "email": "p@x", "roles": ["Admin", "", "ReadOnly"] },
        { "id": 222, "name": "arn:aws:iam::222:user/ana" },
        { "id": "", "name": "sem id", "roles": [] },
    ] });
    assert_eq!(
        accounts(&raw),
        json!([
            { "id": "111", "name": "prod", "email": "p@x", "roles": ["Admin", "ReadOnly"] },
            { "id": "222", "name": "arn:aws:iam::222:user/ana", "email": "", "roles": [] },
        ])
    );
    assert_eq!(accounts(&json!({})), json!([]));
}

#[test]
fn the_open_pulls_and_the_branches_take_the_page_s_shape() {
    let raw = json!([{ "ref": "o/r#3", "number": 3, "title": "PR", "head": "h", "base": "main", "author": "bia", "url": "https://c/3", "draft": true }, { "ref": "" }]);
    let shown = pulls(&raw);
    assert_eq!(shown.as_array().map(Vec::len), Some(1));
    assert_eq!(shown[0]["key"], "o/r#3");
    assert_eq!(shown[0]["draft"], true);
    let listed = branches(&json!([{ "name": "main", "default": true }, { "name": "" }]));
    assert_eq!(listed, json!([{ "name": "main", "default": true }]));
}
