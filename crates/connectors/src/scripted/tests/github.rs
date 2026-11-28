use base64::Engine;
use serde_json::{Value, json};

use super::{creds as scripted_creds, loaded};
use crate::descriptor::AuthSpec;
use crate::error::{ActionError, ErrorClass};
use crate::http::{FakeHttp, HttpResponse, Method};
use crate::idempotency;
use crate::provider::{Credentials, Provider};

fn creds() -> Credentials {
    scripted_creds("ght", json!({ "repos": ["acme/app"] }))
}

async fn call(http: &FakeHttp, action: &str, args: Value, key: &str) -> Result<Value, ActionError> {
    let github = loaded("github");
    let answer = github.call(http, &creds(), action, &args, key).await;
    if let Ok(value) = &answer {
        let spec = github.descriptor().action(action).cloned().unwrap_or_else(|| panic!("no {action}"));
        assert_eq!(crate::descriptor::conforms(&spec.output_schema, value), Ok(()), "{action}: {value}");
    }
    answer
}

fn pull(state: &str, merged_at: Option<&str>) -> Value {
    json!({
        "number": 7, "state": state, "merged_at": merged_at, "html_url": "https://github.com/acme/app/pull/7",
        "title": "T", "head": {"ref": "big/x", "sha": "abc", "repo": {"full_name": "acme/app"}}, "base": {"ref": "main"}, "merge_commit_sha": "m1",
    })
}

#[test]
fn the_device_flow_uses_the_configured_client_and_workflow_scope() {
    let AuthSpec::Device(app) = loaded("github").descriptor().auth else { panic!("not device") };
    assert_eq!(app.client_id, brand::github::CLIENT_ID);
    assert_eq!(app.device_endpoint, "https://github.com/login/device/code");
    assert!(app.scopes.contains(&"workflow".to_owned()));
}

#[test]
fn a_personal_access_token_is_the_alternative_to_the_device_sign_in() {
    let [alternative] = loaded("github")
        .alternatives(domain::Language::Portuguese)
        .try_into()
        .unwrap_or_else(|_| panic!("not one alternative"));
    let AuthSpec::Token(spec) = alternative.auth else { panic!("not a token") };
    assert_eq!(alternative.key, "api_token");
    assert_eq!(spec.access_token(&json!({ "token": "ghp_1" }), domain::Language::Portuguese), Ok("ghp_1".to_owned()));
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
    let answer = call(&http, "code.open_pull_request", args, "k").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!((answer["ref"].clone(), answer["created"].clone()), (json!("acme/app#7"), json!(false)));
    assert_eq!((answer["repo"].as_str(), answer["number"].as_u64()), (Some("acme/app"), Some(7)));
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
    let args = json!({"head": "big/x", "base": "main", "title": "T", "body": "corpo"});
    let answer = call(&http, "code.open_pull_request", args, "k1").await;
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
    assert_eq!(call(&http, "code.open_pull_request", args, "k").await.map(|a| a["created"].clone()), Ok(json!(false)));
}

#[tokio::test]
async fn merging_a_merged_pull_request_is_a_success() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/repos/acme/app/pulls/7",
        200,
        &pull("closed", Some("2024-01-01T00:00:00Z")).to_string(),
    );
    let answer = call(&http, "code.merge", json!({"pr": "acme/app#7", "delete_branch": false}), "k").await;
    assert_eq!(answer, Ok(json!({"merged": true, "already": true, "sha": "m1", "deleted": false})));
    assert!(http.sent_to(Method::Put, "/merge").is_empty());
    assert!(http.sent_to(Method::Delete, "/git/refs").is_empty());
}

#[tokio::test]
async fn merge_deletes_the_head_branch_and_a_failed_delete_keeps_the_merge() {
    let http = FakeHttp::new()
        .once(Method::Get, "/pulls/7", 200, &pull("open", None).to_string())
        .once(Method::Put, "/pulls/7/merge", 200, r#"{"merged":true,"sha":"s9"}"#)
        .once(Method::Delete, "/repos/acme/app/git/refs/heads/big/x", 204, "")
        .once(Method::Get, "/pulls/7", 200, &pull("closed", Some("2024-01-01T00:00:00Z")).to_string())
        .once(Method::Delete, "/git/refs/heads/big/x", 422, r#"{"message":"Reference does not exist"}"#);
    let args = json!({"pr": "acme/app#7"});
    let answer = call(&http, "code.merge", args.clone(), "k").await;
    assert_eq!(answer, Ok(json!({"merged": true, "already": false, "sha": "s9", "deleted": true})));
    let again = call(&http, "code.merge", args, "k").await;
    assert_eq!(again.map(|a| a["deleted"].clone()), Ok(json!(false)));
}

#[tokio::test]
async fn a_branch_from_a_fork_is_not_deleted() {
    let mut forked = pull("open", None);
    forked["head"]["repo"]["full_name"] = json!("someone/app");
    let http = FakeHttp::new().once(Method::Get, "/pulls/7", 200, &forked.to_string()).once(
        Method::Put,
        "/pulls/7/merge",
        200,
        r#"{"merged":true,"sha":"s9"}"#,
    );
    let answer = call(&http, "code.merge", json!({"pr": "acme/app#7"}), "k").await;
    assert_eq!(answer.map(|a| a["deleted"].clone()), Ok(json!(false)));
    assert!(http.sent_to(Method::Delete, "/git/refs").is_empty());
}

#[tokio::test]
async fn merge_sends_the_method_and_the_head_it_saw_and_a_moved_head_is_a_conflict() {
    let http = FakeHttp::new()
        .once(Method::Get, "/pulls/7", 200, &pull("open", None).to_string())
        .once(Method::Put, "/pulls/7/merge", 200, r#"{"merged":true,"sha":"s9"}"#)
        .once(Method::Get, "/pulls/7", 200, &pull("open", None).to_string())
        .once(Method::Put, "/pulls/7/merge", 409, r#"{"message":"Head branch was modified"}"#);
    let args = json!({"pr": "acme/app#7", "method": "squash", "delete_branch": false});
    let answer = call(&http, "code.merge", args.clone(), "k").await;
    assert_eq!(answer, Ok(json!({"merged": true, "already": false, "sha": "s9", "deleted": false})));
    assert_eq!(http.sent()[1].body_json(), json!({"merge_method": "squash", "sha": "abc"}));
    assert_eq!(call(&http, "code.merge", args, "k").await.map_err(|e| e.class), Err(ErrorClass::Conflict));
}

#[tokio::test]
async fn a_pull_request_reads_checks_and_reviews() {
    let checks = r#"{"check_runs":[{"name":"ci","status":"completed","conclusion":"success"},{"name":"lint","status":"in_progress","conclusion":null}]}"#;
    let reviews = json!([{"id": 1, "user": {"login": "ana"}, "state": "APPROVED",
        "body": idempotency::marked("ok", "k/review/0/1"), "submitted_at": "2024-01-01T00:00:00Z"}])
    .to_string();
    let http = FakeHttp::new()
        .once(Method::Get, "/pulls/7", 200, &pull("open", None).to_string())
        .once(Method::Get, "/commits/abc/check-runs", 200, checks)
        .once(Method::Get, "/pulls/7/reviews", 200, &reviews);
    let answer = call(&http, "code.pull_request", json!({"pr": "acme/app#7"}), "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["checks"][0], json!({"name": "ci", "state": "passed", "url": null}));
    assert_eq!(answer["checks"][1]["state"], "running");
    assert_eq!(answer["reviews"][0]["author"], "ana");
    assert_eq!(answer["reviews"][0]["verdict"], "approve");
    assert_eq!(answer["reviews"][0]["body"], "ok");
    assert_eq!((answer["state"].as_str(), answer["ref"].as_str()), (Some("open"), Some("acme/app#7")));
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
    let answer = call(&http, "code.submit_review", json!({"pr": "acme/app#7", "verdict": "approve"}), "rk").await;
    assert_eq!(answer, Ok(json!({"id": "3", "verdict": "approve", "created": false})));
}

#[tokio::test]
async fn a_verdict_on_one_s_own_pull_request_goes_as_a_comment_with_the_same_text() {
    let refused =
        r#"{"message":"Unprocessable Entity","errors":["Review Can not request changes on your own pull request"]}"#;
    let http = FakeHttp::new()
        .once(Method::Get, "/pulls/7/reviews", 200, "[]")
        .once(Method::Post, "/pulls/7/reviews", 422, refused)
        .once(Method::Post, "/pulls/7/reviews", 200, r#"{"id":5,"state":"COMMENTED"}"#);
    let args = json!({"pr": "acme/app#7", "verdict": "request_changes", "body": "corrija",
                      "lines": [{"path": "src/a.rs", "line": 3, "body": "aqui"}]});
    let answer = call(&http, "code.submit_review", args, "rk3").await;
    assert_eq!(answer, Ok(json!({"id": "5", "verdict": "request_changes", "created": true, "as": "comment"})));
    let again = http.sent()[2].body_json();
    assert_eq!(again["event"], "COMMENT");
    assert!(again["body"].as_str().is_some_and(|body| body.starts_with("corrija")));
    assert_eq!(again["comments"][0]["body"], "aqui");
}

#[tokio::test]
async fn a_review_with_lines_is_submitted_with_its_event() {
    let http = FakeHttp::new().once(Method::Get, "/pulls/7/reviews", 200, "[]").once(
        Method::Post,
        "/pulls/7/reviews",
        200,
        r#"{"id":4,"state":"CHANGES_REQUESTED"}"#,
    );
    let args = json!({"pr": "acme/app#7", "verdict": "request_changes", "body": "veja",
                      "lines": [{"path": "src/a.rs", "line": 3, "body": "aqui"}]});
    let answer = call(&http, "code.submit_review", args, "rk2").await;
    assert_eq!(answer.map(|a| a["created"].clone()), Ok(json!(true)));
    let sent = http.sent()[1].body_json();
    assert_eq!(sent["event"], "REQUEST_CHANGES");
    assert_eq!(sent["comments"][0], json!({"path": "src/a.rs", "line": 3, "body": "aqui", "side": "RIGHT"}));
    let wrong = call(&http, "code.submit_review", json!({"pr": "acme/app#7", "verdict": "nah"}), "").await;
    assert_eq!(wrong.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn an_agent_s_review_comes_back_with_the_agent_as_its_author() {
    let http = FakeHttp::new().once(Method::Post, "/pulls/7/reviews", 200, r#"{"id":5,"state":"COMMENTED"}"#);
    let args = json!({"pr": "acme/app#7", "verdict": "comment", "agent": "reviewer",
                      "lines": [{"path": "src/a.rs", "line": 3, "body": "aqui"}]});
    assert!(call(&http, "code.submit_review", args, "").await.is_ok());
    let said = http.sent()[0].body_json()["comments"][0]["body"].as_str().unwrap_or_default().to_owned();
    let threads = json!({"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [
        {"id": "T1", "isResolved": false, "isOutdated": false, "path": "src/a.rs", "line": 3,
         "comments": {"nodes": [{"id": "C1", "body": said, "createdAt": "x", "author": {"login": "ana"}}]}}
    ]}}}}});
    let http = FakeHttp::new().once(Method::Post, "/graphql", 200, &threads.to_string());
    let read = call(&http, "code.review_comments", json!({"pr": "acme/app#7"}), "").await;
    let Ok(read) = read else { panic!("{read:?}") };
    assert_eq!((read[0]["agent"].as_str(), read[0]["body"].as_str()), (Some("reviewer"), Some("aqui")));
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
    let read = call(&http, "code.review_comments", json!({"pr": "acme/app#7"}), "").await;
    let Ok(read) = read else { panic!("{read:?}") };
    assert_eq!((read[0]["author"].as_str(), read[0]["thread"].as_str()), (Some("ana"), Some("T1")));
    assert_eq!(
        (read[0]["path"].as_str(), read[0]["line"].as_u64(), read[0]["resolved"].as_bool()),
        (Some("a.rs"), Some(2), Some(false))
    );
    assert_eq!(http.sent()[0].body_json()["variables"], json!({"owner": "acme", "name": "app", "number": 7}));
    let resolved = call(&http, "code.resolve_thread", json!({"thread": "T1"}), "k").await;
    assert_eq!(resolved, Ok(json!({"thread": "T1", "resolved": true})));
    let missing = call(&http, "code.resolve_thread", json!({"thread": "T9"}), "k").await;
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
    let answer = call(&http, "code.files_diff", json!({"pr": 7}), "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!((answer[0]["path"].as_str(), answer[0]["change"].as_str()), (Some("a.rs"), Some("modified")));
}

#[tokio::test]
async fn a_bad_repo_never_reaches_the_api() {
    let http = FakeHttp::new();
    let answer = call(&http, "code.pull_request", json!({"pr": "../x#1"}), "").await;
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
    let answer = call(&http, "pipelines.run", json!({"run": "acme/app/runs/5"}), "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!((answer["ref"].as_str(), answer["state"].as_str()), (Some("acme/app/runs/5"), Some("running")));
    assert_eq!(
        (answer["jobs"][0]["state"].as_str(), answer["jobs"][1]["state"].as_str()),
        (Some("passed"), Some("queued"))
    );
    assert_eq!(answer["jobs"][0]["steps"][0]["state"], "passed");
    assert_eq!(answer["jobs"][0]["steps"][0]["name"], "checkout");
    assert_eq!(answer["jobs"][1]["key"], "test");
    assert_eq!(answer["jobs"][1]["needs"], json!(["build"]));
}

#[tokio::test]
async fn runs_filter_by_status_for_the_last_good_deploy() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/actions/workflows/deployment.yml/runs?per_page=1&branch=release&status=success",
        200,
        r#"{"workflow_runs":[{"id":4,"status":"completed","conclusion":"success","head_sha":"abc"}]}"#,
    );
    let args = json!({"pipeline": "deployment.yml", "ref": "release", "status": "success", "limit": 1});
    let answer = call(&http, "pipelines.runs", args, "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer[0]["ref"], "acme/app/runs/4");
}

#[tokio::test]
async fn merging_a_branch_into_another_reports_when_there_was_nothing_to_merge() {
    let http = FakeHttp::new().once(Method::Post, "/repos/acme/app/merges", 201, r#"{"sha":"f00"}"#).once(
        Method::Post,
        "/repos/acme/app/merges",
        204,
        "",
    );
    let args = json!({"base": "release", "head": "develop"});
    let first = call(&http, "code.merge_branches", args.clone(), "m1").await;
    assert_eq!(first, Ok(json!({"merged": true, "already": false, "sha": "f00"})));
    let again = call(&http, "code.merge_branches", args, "m1").await;
    assert_eq!(again, Ok(json!({"merged": true, "already": true})));
    let bad = call(&http, "code.merge_branches", json!({"base": "release", "head": "a..b"}), "m2").await;
    assert!(bad.is_err());
}

#[tokio::test]
async fn runs_filter_by_branch_or_sha() {
    let http = FakeHttp::new()
        .once(
            Method::Get,
            "/actions/runs?per_page=5&branch=big%2Fx",
            200,
            r#"{"workflow_runs":[{"id":1,"status":"completed","triggering_actor":{"login":"dependabot[bot]"}}]}"#,
        )
        .once(Method::Get, "/actions/workflows/ci.yml/runs?per_page=20&head_sha=", 200, r#"{"workflow_runs":[]}"#)
        .once(Method::Get, "/actions/runs?per_page=1", 200, r#"{"workflow_runs":[]}"#);
    let answer = call(&http, "pipelines.runs", json!({"ref": "big/x", "limit": 5}), "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!((answer[0]["ref"].as_str(), answer[0]["state"].as_str()), (Some("acme/app/runs/1"), Some("failed")));
    assert_eq!(answer[0]["actor"], "dependabot[bot]");
    let sha = "a".repeat(40);
    let answer = call(&http, "pipelines.runs", json!({"ref": sha, "pipeline": "ci.yml"}), "").await;
    assert_eq!(answer, Ok(json!([])));
    let whole = call(&http, "pipelines.runs", json!({"limit": 1}), "").await;
    assert_eq!(whole, Ok(json!([])), "without a ref it lists the repository's runs");
}

#[tokio::test]
async fn a_job_log_keeps_the_tail() {
    let http = FakeHttp::new().once(Method::Get, "/actions/jobs/10/logs", 200, "a\nb\nc\nd");
    let args = json!({"run": "acme/app/runs/5", "job": "10", "tail": 2});
    let answer = call(&http, "pipelines.job_log", args, "").await;
    assert_eq!(answer, Ok(json!({"text": "c\nd", "truncated": true})));
}

#[tokio::test]
async fn a_deployment_is_reviewed_once_and_nothing_pending_is_already_done() {
    let pending = r#"[{"environment":{"id":42,"name":"production"},"current_user_can_approve":true,"wait_timer":0},
                      {"environment":{"id":43,"name":"staging"},"current_user_can_approve":true,"wait_timer":0}]"#;
    let http = FakeHttp::new()
        .once(Method::Get, "/actions/runs/5/pending_deployments", 200, pending)
        .once(Method::Post, "/actions/runs/5/pending_deployments", 200, "[]")
        .once(Method::Get, "/actions/runs/5/pending_deployments", 200, "[]");
    let args = json!({"run": "acme/app/runs/5", "decision": "approve", "deployments": ["production"], "comment": "ok"});
    let answer = call(&http, "pipelines.review_deployment", args.clone(), "k").await;
    assert_eq!(answer, Ok(json!({"decision": "approve", "deployments": ["42"], "already": false})));
    assert_eq!(http.sent()[1].body_json(), json!({"environment_ids": [42], "state": "approved", "comment": "ok"}));
    let again = call(&http, "pipelines.review_deployment", args, "k").await;
    assert_eq!(again, Ok(json!({"decision": "approve", "deployments": [], "already": true})));
}

#[tokio::test]
async fn a_rate_limited_answer_is_classified_with_its_wait() {
    let limited = HttpResponse::new(429, "{}").with_header("retry-after", "12");
    let http = FakeHttp::new().respond(Method::Get, "/pulls/7", limited);
    let answer = call(&http, "code.merge", json!({"pr": "acme/app#7"}), "k").await;
    let wait = std::time::Duration::from_secs(12);
    assert_eq!(answer.map_err(|e| e.class), Err(ErrorClass::RateLimited { retry_after: wait }));
}

#[tokio::test]
async fn events_come_from_pulls_reviews_and_runs_changed_since_the_cursor_and_etags_are_kept() {
    let github = loaded("github");
    let source = github.events().unwrap_or_else(|| panic!("no events"));
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
    let batch = source.poll(&http, &creds(), Some(cursor), &[]).await;
    let Ok(batch) = batch else { panic!("{batch:?}") };
    let kinds: Vec<&str> = batch.events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["github.run.finished", "github.review.submitted", "github.pr.merged"]);
    assert_eq!(batch.events[2].subject, "acme/app#7");
    assert_eq!(batch.events[2].data["state"], "merged");
    assert_eq!(batch.events[0].subject, "acme/app/runs/5");
    assert_eq!(
        (batch.events[0].data["run"].as_str(), batch.events[0].data["state"].as_str()),
        (Some("acme/app/runs/5"), Some("failed"))
    );
    assert_eq!(batch.events[0].capability.as_str(), "pipelines");
    let next: Value = serde_json::from_str(&batch.cursor).unwrap_or_else(|_| panic!("bad cursor"));
    assert_eq!(next["since"], "2024-01-02T05:00:00Z");
    let http = FakeHttp::new().once(Method::Get, "/pulls?state=all", 304, "").once(
        Method::Get,
        "/actions/runs?per_page=50",
        304,
        "",
    );
    let quiet = source.poll(&http, &creds(), Some(&batch.cursor), &[]).await;
    assert!(quiet.is_ok_and(|b| b.events.is_empty()));
    assert_eq!(http.sent()[0].header_value("if-none-match"), Some("\"p1\""));
}

fn repos_page(from: u64, count: u64) -> String {
    let repos: Vec<Value> = (from..from + count)
        .map(|n| {
            json!({ "id": n, "full_name": format!("acme/r{n}"), "name": format!("r{n}"), "private": n % 2 == 0,
            "html_url": format!("https://github.com/acme/r{n}"), "default_branch": "main" })
        })
        .collect();
    Value::Array(repos).to_string()
}

#[tokio::test]
async fn the_repositories_are_listed_page_by_page_until_a_short_page() {
    let http = FakeHttp::new().once(Method::Get, "/user/repos?per_page=100&page=1&", 200, &repos_page(0, 100)).once(
        Method::Get,
        "/user/repos?per_page=100&page=2&",
        200,
        &repos_page(100, 3),
    );
    let answer = call(&http, "code.repos", json!({}), "k").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    let listed = answer.as_array().expect("list");
    assert_eq!(listed.len(), 103);
    assert_eq!(
        listed[101],
        json!({ "id": "101", "ref": "acme/r101", "name": "r101", "url": "https://github.com/acme/r101",
        "default_branch": "main", "private": false })
    );
    assert_eq!(http.sent().len(), 2);
}

#[tokio::test]
async fn the_repositories_can_be_filtered_by_name() {
    let http = FakeHttp::new().once(Method::Get, "/user/repos?per_page=100&page=1&", 200, &repos_page(0, 12));
    let answer = call(&http, "code.repos", json!({ "query": "R1" }), "k").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    let names: Vec<&str> = answer.as_array().expect("list").iter().filter_map(|repo| repo["ref"].as_str()).collect();
    assert_eq!(names, ["acme/r1", "acme/r10", "acme/r11"]);
}

#[tokio::test]
async fn the_suggestion_for_a_folder_is_the_exact_remote_or_else_the_repository_named_like_it() {
    let http = FakeHttp::new().always(Method::Get, "/user/repos?per_page=100&page=1&", 200, &repos_page(0, 12));
    let exact = call(&http, "code.options", json!({ "names": ["acme/r10", "r1"] }), "k").await;
    assert_eq!(exact, Ok(json!([{ "value": "acme/r10", "label": "acme/r10" }])));
    let short = call(&http, "code.options", json!({ "names": ["other/r11", "r11"] }), "k").await;
    assert_eq!(short, Ok(json!([{ "value": "acme/r11", "label": "acme/r11" }])));
    let none = call(&http, "code.options", json!({ "names": ["nada"] }), "k").await;
    assert_eq!(none, Ok(json!([])));
    assert_eq!(http.sent().len(), 3);
}

#[tokio::test]
async fn the_open_pull_requests_of_a_repository_are_listed() {
    let http = FakeHttp::new().once(
        Method::Get,
        "/repos/acme/app/pulls?state=open",
        200,
        &json!([pull("open", None)]).to_string(),
    );
    let answer = call(&http, "code.pull_requests", json!({ "repo": "acme/app" }), "k").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer[0]["ref"], json!("acme/app#7"));
    assert_eq!(answer[0]["head"], json!("big/x"));
}

#[tokio::test]
async fn the_branches_mark_the_default_one() {
    let http = FakeHttp::new()
        .once(Method::Get, "/repos/acme/app/branches", 200, &json!([{ "name": "dev" }, { "name": "main" }]).to_string())
        .once(Method::Get, "/repos/acme/app", 200, &json!({ "default_branch": "main" }).to_string());
    let answer = call(&http, "code.branches", json!({}), "k").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer, json!([{ "name": "dev", "default": false }, { "name": "main", "default": true }]));
}
#[tokio::test]
async fn the_account_is_the_login_and_an_unknown_action_is_refused() {
    let http = FakeHttp::new().once(Method::Get, "/user", 200, r#"{"login":"ana"}"#);
    let accounts = loaded("github").accounts(&http, &creds()).await;
    let Ok([account]) = accounts.as_deref() else { panic!("{accounts:?}") };
    assert_eq!((account.id.as_str(), account.label.as_str()), ("ana", "ana"));
    assert_eq!(account.settings, json!({ "login": "ana" }));
    let refused = loaded("github").call(&http, &creds(), "tracker.issue", &json!({}), "k").await;
    assert_eq!(refused.map_err(|e| e.class), Err(ErrorClass::Permanent));
}

#[tokio::test]
async fn refs_name_the_repository_and_fall_back_to_the_connection_s_repository() {
    let pull = json!({"number": 12, "state": "open", "head": {"sha": "s"}});
    let http = FakeHttp::new()
        .always(Method::Get, "/repos/o/r/pulls/12", 200, &pull.to_string())
        .always(Method::Get, "/repos/acme/app/pulls/7", 200, &pull.to_string())
        .always(Method::Get, "/check-runs", 200, r#"{"check_runs":[]}"#)
        .always(Method::Get, "/reviews", 200, "[]");
    let named = call(&http, "code.pull_request", json!({ "pr": "o/r#12" }), "").await;
    assert_eq!(named.map(|a| a["repo"].clone()), Ok(json!("o/r")));
    let bare = call(&http, "code.pull_request", json!({ "pr": 7 }), "").await;
    assert_eq!(bare.map(|a| a["repo"].clone()), Ok(json!("acme/app")));
    assert!(call(&http, "pipelines.run", json!({ "run": "o/r/runs/x" }), "").await.is_err());
}

#[tokio::test]
async fn the_provider_s_states_and_every_yaml_form_of_needs_are_read() {
    let yaml = "name: ci\non:\n  push:\n\njobs:\n  build:\n    name: Build app\n    runs-on: ubuntu-latest\n  test:\n    needs: build\n  # comment\n  deploy:\n    needs: [build, \"test\"]\n  notify:\n    needs:\n      - deploy\n      - test\nenv:\n  A: 1\n";
    let content = base64::engine::general_purpose::STANDARD.encode(yaml);
    let run = json!({"id": 5, "path": ".github/workflows/ci.yml", "head_sha": "abc", "status": "completed", "conclusion": "timed_out"});
    let jobs = json!({"jobs": [
        {"id": 1, "name": "Build app", "status": "completed", "conclusion": "neutral"},
        {"id": 2, "name": "test (a)", "status": "waiting"},
        {"id": 3, "name": "deploy / call", "status": "requested"},
        {"id": 4, "name": "notify", "status": "completed", "conclusion": "success"}
    ]});
    let http = FakeHttp::new()
        .once(Method::Get, "/actions/runs/5/jobs", 200, &jobs.to_string())
        .once(Method::Get, "/actions/runs/5", 200, &run.to_string())
        .once(Method::Get, "/contents/", 200, &json!({"content": content}).to_string());
    let answer = call(&http, "pipelines.run", json!({"run": "acme/app/runs/5"}), "").await;
    let Ok(answer) = answer else { panic!("{answer:?}") };
    assert_eq!(answer["state"], "failed");
    let shown: Vec<(String, String, Value)> = answer["jobs"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|job| {
            (
                job["key"].as_str().unwrap_or_default().to_owned(),
                job["state"].as_str().unwrap_or_default().to_owned(),
                job["needs"].clone(),
            )
        })
        .collect();
    let expected = [
        ("build", "skipped", json!([])),
        ("test", "waiting", json!(["build"])),
        ("deploy", "queued", json!(["build", "test"])),
        ("notify", "passed", json!(["deploy", "test"])),
    ];
    let expected: Vec<(String, String, Value)> =
        expected.into_iter().map(|(key, state, needs)| (key.to_owned(), state.to_owned(), needs)).collect();
    assert_eq!(shown, expected);
}
