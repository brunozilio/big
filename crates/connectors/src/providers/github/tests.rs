use base64::Engine;
use domain::connection::ConnectionId;
use serde_json::json;

use super::*;
use crate::error::ErrorClass;
use crate::http::{FakeHttp, HttpResponse, Method};
use crate::idempotency;

fn creds() -> Credentials {
    Credentials::new(&ConnectionId::from("gh1"), "ght", json!({ "repos": ["acme/app"] }))
}

async fn call(http: &FakeHttp, action: &str, args: Value, key: &str) -> Result<Value, ActionError> {
    Github.call(http, &creds(), action, &args, key).await
}

fn pull(state: &str, merged_at: Option<&str>) -> Value {
    json!({
        "number": 7, "state": state, "merged_at": merged_at, "html_url": "https://github.com/acme/app/pull/7",
        "title": "T", "head": {"ref": "big/x", "sha": "abc"}, "base": {"ref": "main"}, "merge_commit_sha": "m1",
    })
}

#[test]
fn the_device_flow_uses_the_configured_client_and_workflow_scope() {
    let AuthSpec::Device(app) = Github.descriptor().auth else { panic!("not device") };
    assert_eq!(app.client_id, brand::connectors::GITHUB_CLIENT_ID);
    assert!(app.scopes.contains(&"workflow".to_owned()));
}

#[tokio::test]
async fn an_open_pull_request_of_the_branch_is_reused() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/repos/acme/app/pulls?head=acme%3Abig%2Fx",
        200,
        &json!([pull("open", None)]).to_string(),
    );
    let args = json!({"repo": "acme/app", "head": "big/x", "base": "main", "title": "T"});
    let answer = call(&http, code_actions::OPEN_PULL_REQUEST, args, "k").await;
    assert_eq!(answer.map(|a| (a["number"].clone(), a["created"].clone())), Ok((json!(7), json!(false))));
    assert_eq!(http.sent().len(), 1);
    assert_eq!(http.sent()[0].header_value("authorization"), Some("Bearer ght"));
}

#[tokio::test]
async fn a_new_pull_request_carries_the_invisible_marker() {
    let http = FakeHttp::new().once(Method::Get, "/pulls?head=", 200, "[]").once(
        Method::Post,
        "/repos/acme/app/pulls",
        201,
        &pull("open", None).to_string(),
    );
    let args = json!({"repo": "acme/app", "head": "big/x", "base": "main", "title": "T", "body": "corpo"});
    let answer = call(&http, code_actions::OPEN_PULL_REQUEST, args, "k1").await;
    assert_eq!(answer.map(|a| a["created"].clone()), Ok(json!(true)));
    let sent = http.sent()[1].body_json();
    assert!(idempotency::carries(sent["body"].as_str().unwrap_or_default(), "k1"));
    assert_eq!(sent["head"], "big/x");
}

#[tokio::test]
async fn a_race_on_create_finds_the_pull_request_that_won() {
    let http = FakeHttp::new()
        .once(Method::Get, "/pulls?head=", 200, "[]")
        .once(Method::Post, "/pulls", 422, r#"{"message":"A pull request already exists for acme:big/x."}"#)
        .once(Method::Get, "/pulls?head=", 200, &json!([pull("open", None)]).to_string());
    let args = json!({"repo": "acme/app", "head": "big/x", "base": "main", "title": "T"});
    assert_eq!(
        call(&http, code_actions::OPEN_PULL_REQUEST, args, "k").await.map(|a| a["created"].clone()),
        Ok(json!(false))
    );
}

#[tokio::test]
async fn merging_a_merged_pull_request_is_a_success() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/repos/acme/app/pulls/7",
        200,
        &pull("closed", Some("2024-01-01T00:00:00Z")).to_string(),
    );
    let answer = call(&http, code_actions::MERGE, json!({"repo": "acme/app", "number": 7}), "k").await;
    assert_eq!(answer, Ok(json!({"merged": true, "already": true, "sha": "m1"})));
    assert!(http.sent_to(Method::Put, "/merge").is_empty());
}

#[tokio::test]
async fn merge_sends_the_method_and_the_head_it_saw_and_a_moved_head_is_a_conflict() {
    let http = FakeHttp::new()
        .once(Method::Get, "/pulls/7", 200, &pull("open", None).to_string())
        .once(Method::Put, "/pulls/7/merge", 200, r#"{"merged":true,"sha":"s9"}"#)
        .once(Method::Get, "/pulls/7", 200, &pull("open", None).to_string())
        .once(Method::Put, "/pulls/7/merge", 409, r#"{"message":"Head branch was modified"}"#);
    let args = json!({"repo": "acme/app", "number": 7, "method": "squash"});
    let answer = call(&http, code_actions::MERGE, args.clone(), "k").await;
    assert_eq!(answer, Ok(json!({"merged": true, "already": false, "sha": "s9"})));
    assert_eq!(http.sent()[1].body_json(), json!({"merge_method": "squash", "sha": "abc"}));
    assert_eq!(call(&http, code_actions::MERGE, args, "k").await.map_err(|e| e.class), Err(ErrorClass::Conflict));
}

#[tokio::test]
async fn a_pull_request_reads_checks_and_reviews() {
    let checks = r#"{"check_runs":[{"name":"ci","status":"completed","conclusion":"success"},{"name":"lint","status":"in_progress","conclusion":null}]}"#;
    let reviews =
        r#"[{"id":1,"user":{"login":"ana"},"state":"APPROVED","body":"ok","submitted_at":"2024-01-01T00:00:00Z"}]"#;
    let http = FakeHttp::new()
        .once(Method::Get, "/pulls/7", 200, &pull("open", None).to_string())
        .once(Method::Get, "/commits/abc/check-runs", 200, checks)
        .once(Method::Get, "/pulls/7/reviews", 200, reviews);
    let answer = call(&http, code_actions::PULL_REQUEST, json!({"repo": "acme/app", "number": 7}), "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["checks"]["state"], "pending");
    assert_eq!(answer["reviews"][0]["user"], "ana");
    assert_eq!(answer["state"], "open");
}

#[tokio::test]
async fn a_review_already_carrying_the_key_is_not_submitted_again() {
    let body = idempotency::marked("lgtm", "rk");
    let http = FakeHttp::new().once(
        Method::Get,
        "/pulls/7/reviews",
        200,
        &json!([{"id": 3, "state": "APPROVED", "body": body}]).to_string(),
    );
    let answer =
        call(&http, code_actions::SUBMIT_REVIEW, json!({"repo": "acme/app", "number": 7, "event": "approve"}), "rk")
            .await;
    assert_eq!(answer, Ok(json!({"id": 3, "state": "APPROVED", "created": false})));
}

#[tokio::test]
async fn a_review_with_lines_is_submitted_with_its_event() {
    let http = FakeHttp::new().once(Method::Get, "/pulls/7/reviews", 200, "[]").once(
        Method::Post,
        "/pulls/7/reviews",
        200,
        r#"{"id":4,"state":"CHANGES_REQUESTED"}"#,
    );
    let args = json!({"repo": "acme/app", "number": 7, "event": "request_changes", "body": "veja",
                      "comments": [{"path": "src/a.rs", "line": 3, "body": "aqui"}]});
    let answer = call(&http, code_actions::SUBMIT_REVIEW, args, "rk2").await;
    assert_eq!(answer.map(|a| a["created"].clone()), Ok(json!(true)));
    let sent = http.sent()[1].body_json();
    assert_eq!(sent["event"], "REQUEST_CHANGES");
    assert_eq!(sent["comments"][0], json!({"path": "src/a.rs", "line": 3, "body": "aqui", "side": "RIGHT"}));
    let wrong =
        call(&http, code_actions::SUBMIT_REVIEW, json!({"repo": "acme/app", "number": 7, "event": "nah"}), "").await;
    assert_eq!(wrong.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn review_threads_and_their_resolution_go_through_graphql() {
    let threads = json!({"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [
        {"id": "T1", "isResolved": false, "isOutdated": false, "path": "a.rs", "line": 2,
         "comments": {"nodes": [{"id": "C1", "body": "x", "createdAt": "2024-01-01T00:00:00Z", "author": {"login": "ana"}}]}}
    ]}}}}});
    let http = FakeHttp::new()
        .once(Method::Post, "/graphql", 200, &threads.to_string())
        .once(
            Method::Post,
            "/graphql",
            200,
            r#"{"data":{"resolveReviewThread":{"thread":{"id":"T1","isResolved":true}}}}"#,
        )
        .once(Method::Post, "/graphql", 200, r#"{"errors":[{"message":"Could not resolve to a node"}]}"#);
    let read = call(&http, code_actions::REVIEW_COMMENTS, json!({"repo": "acme/app", "number": 7}), "").await;
    assert_eq!(read.map(|r| r["threads"][0]["comments"][0]["author"].clone()), Ok(json!("ana")));
    assert_eq!(http.sent()[0].body_json()["variables"], json!({"owner": "acme", "name": "app", "number": 7}));
    let resolved = call(&http, code_actions::RESOLVE_THREAD, json!({"thread": "T1"}), "k").await;
    assert_eq!(resolved, Ok(json!({"resolved": true})));
    let missing = call(&http, code_actions::RESOLVE_THREAD, json!({"thread": "T9"}), "k").await;
    assert_eq!(missing.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn files_diff_lists_every_changed_file() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/pulls/7/files?per_page=100&page=1",
        200,
        r#"[{"filename":"a.rs","status":"modified","additions":1,"deletions":2,"patch":"@@"}]"#,
    );
    let answer = call(&http, code_actions::FILES_DIFF, json!({"repo": "acme/app", "number": 7}), "").await;
    assert_eq!(answer.map(|a| a["files"][0]["path"].clone()), Ok(json!("a.rs")));
}

#[tokio::test]
async fn a_bad_repo_never_reaches_the_api() {
    let http = FakeHttp::new();
    let answer = call(&http, code_actions::PULL_REQUEST, json!({"repo": "../x", "number": 1}), "").await;
    assert_eq!(answer.map_err(|e| e.class), Err(ErrorClass::Permanent));
    assert!(http.sent().is_empty());
}

#[tokio::test]
async fn a_run_is_the_job_graph_with_needs_and_steps() {
    let yaml = "jobs:\n  build:\n    runs-on: x\n  test:\n    name: Test\n    needs: build\n";
    let content = base64::engine::general_purpose::STANDARD.encode(yaml);
    let run = json!({"id": 5, "path": ".github/workflows/ci.yml", "head_sha": "abc", "status": "in_progress"});
    let jobs = json!({"jobs": [
        {"id": 10, "name": "build", "status": "completed", "conclusion": "success", "steps": [{"number": 1, "name": "checkout", "status": "completed", "conclusion": "success"}]},
        {"id": 11, "name": "Test (linux)", "status": "queued", "conclusion": null, "steps": []}
    ]});
    let http = FakeHttp::new()
        .once(Method::Get, "/actions/runs/5/jobs", 200, &jobs.to_string())
        .once(Method::Get, "/actions/runs/5", 200, &run.to_string())
        .once(Method::Get, "/contents/.github/workflows/ci.yml?ref=abc", 200, &json!({"content": content}).to_string());
    let answer = call(&http, pipeline_actions::RUN, json!({"repo": "acme/app", "run": 5}), "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["jobs"][0]["steps"][0]["name"], "checkout");
    assert_eq!(answer["jobs"][1]["key"], "test");
    assert_eq!(answer["jobs"][1]["needs"], json!(["build"]));
}

#[tokio::test]
async fn runs_filter_by_branch_or_sha() {
    let http = FakeHttp::new()
        .once(
            Method::Get,
            "/actions/runs?per_page=5&branch=big%2Fx",
            200,
            r#"{"workflow_runs":[{"id":1,"status":"completed"}]}"#,
        )
        .once(Method::Get, "/actions/workflows/ci.yml/runs?per_page=20&head_sha=", 200, r#"{"workflow_runs":[]}"#);
    let answer = call(&http, pipeline_actions::RUNS, json!({"repo": "acme/app", "ref": "big/x", "limit": 5}), "").await;
    assert_eq!(answer.map(|a| a["runs"][0]["id"].clone()), Ok(json!(1)));
    let sha = "a".repeat(40);
    let answer =
        call(&http, pipeline_actions::RUNS, json!({"repo": "acme/app", "ref": sha, "workflow": "ci.yml"}), "").await;
    assert_eq!(answer, Ok(json!({"runs": []})));
}

#[tokio::test]
async fn a_job_log_keeps_the_tail() {
    let http = FakeHttp::new().once(Method::Get, "/actions/jobs/10/logs", 200, "a\nb\nc\nd");
    let answer = call(&http, pipeline_actions::JOB_LOG, json!({"repo": "acme/app", "job": 10, "tail": 2}), "").await;
    assert_eq!(answer, Ok(json!({"log": "c\nd", "truncated": true})));
}

#[tokio::test]
async fn a_deployment_is_reviewed_once_and_nothing_pending_is_already_done() {
    let pending = r#"[{"environment":{"id":42,"name":"production"},"current_user_can_approve":true,"wait_timer":0},
                      {"environment":{"id":43,"name":"staging"},"current_user_can_approve":true,"wait_timer":0}]"#;
    let http = FakeHttp::new()
        .once(Method::Get, "/actions/runs/5/pending_deployments", 200, pending)
        .once(Method::Post, "/actions/runs/5/pending_deployments", 200, "[]")
        .once(Method::Get, "/actions/runs/5/pending_deployments", 200, "[]");
    let args =
        json!({"repo": "acme/app", "run": 5, "decision": "approve", "environments": ["production"], "comment": "ok"});
    let answer = call(&http, pipeline_actions::REVIEW_DEPLOYMENT, args.clone(), "k").await;
    assert_eq!(answer, Ok(json!({"reviewed": ["production"], "already": false})));
    assert_eq!(http.sent()[1].body_json(), json!({"environment_ids": [42], "state": "approved", "comment": "ok"}));
    let again = call(&http, pipeline_actions::REVIEW_DEPLOYMENT, args, "k").await;
    assert_eq!(again, Ok(json!({"reviewed": [], "already": true})));
}

#[tokio::test]
async fn a_rate_limited_answer_is_classified_with_its_wait() {
    let limited = HttpResponse::new(429, "{}").with_header("retry-after", "12");
    let http = FakeHttp::new().respond(Method::Get, "/pulls/7", limited);
    let answer = call(&http, code_actions::MERGE, json!({"repo": "acme/app", "number": 7}), "k").await;
    let wait = std::time::Duration::from_secs(12);
    assert_eq!(answer.map_err(|e| e.class), Err(ErrorClass::RateLimited { retry_after: wait }));
}

#[tokio::test]
async fn events_come_from_pulls_reviews_and_runs_changed_since_the_cursor_and_etags_are_kept() {
    let source = Github.events().unwrap_or_else(|| panic!("no events"));
    let cursor = r#"{"since":"2024-01-02T00:00:00Z","etags":{}}"#;
    let pulls = json!([
        {"number": 7, "state": "closed", "merged_at": "2024-01-02T05:00:00Z", "updated_at": "2024-01-02T05:00:00Z", "head": {"ref": "big/x"}},
        {"number": 6, "state": "open", "merged_at": null, "updated_at": "2024-01-01T00:00:00Z"}
    ]);
    let reviews = r#"[{"id":99,"state":"APPROVED","user":{"login":"ana"},"submitted_at":"2024-01-02T04:00:00Z"}]"#;
    let runs = r#"{"workflow_runs":[{"id":5,"run_attempt":1,"status":"completed","conclusion":"failure","updated_at":"2024-01-02T03:00:00Z"},
                                    {"id":6,"run_attempt":1,"status":"in_progress","updated_at":"2024-01-02T03:00:00Z"}]}"#;
    let http = FakeHttp::new()
        .respond(
            Method::Get,
            "/pulls?state=all",
            HttpResponse::new(200, pulls.to_string()).with_header("etag", "\"p1\""),
        )
        .once(Method::Get, "/pulls/7/reviews", 200, reviews)
        .once(Method::Get, "/actions/runs?per_page=50", 200, runs);
    let batch = source.poll(&http, &creds(), Some(cursor)).await;
    let Ok(batch) = batch else { panic!("{batch:?}") };
    let kinds: Vec<&str> = batch.events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["pipelines.run.finished", "code.review.submitted", "code.pr.merged"]);
    assert_eq!(batch.events[2].subject, "acme/app#7");
    assert_eq!(batch.events[0].capability.as_str(), "pipelines");
    let next: events::Cursor = serde_json::from_str(&batch.cursor).unwrap_or_else(|_| panic!("bad cursor"));
    assert_eq!(next.since.to_string(), "2024-01-02T05:00:00Z");
    let http = FakeHttp::new().once(Method::Get, "/pulls?state=all", 304, "").once(
        Method::Get,
        "/actions/runs?per_page=50",
        304,
        "",
    );
    let quiet = source.poll(&http, &creds(), Some(&batch.cursor)).await;
    assert!(quiet.is_ok_and(|b| b.events.is_empty()));
    assert_eq!(http.sent()[0].header_value("if-none-match"), Some("\"p1\""));
}
