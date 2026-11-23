use super::*;

#[test]
fn a_reference_names_the_repository_and_the_number() {
    assert_eq!(reference("o/r#12", &json!({})).ok(), Some(("o/r".to_owned(), 12)));
    assert_eq!(reference("7", &json!({ "repo": "a/b" })).ok(), Some(("a/b".to_owned(), 7)));
    assert!(reference("7", &json!({})).is_err());
    assert!(reference("o/r#x", &json!({})).is_err());
}

#[test]
fn an_issue_takes_the_page_s_shape() {
    let raw = json!({
        "key": "K-1", "url": "https://t/K-1", "title": "T", "status": "A fazer", "description": "d",
        "subtasks": [{ "key": "K-2", "title": "s", "status": "Done" }],
        "attachments": [{ "name": "a.png", "size": 10, "url": "https://t/a" }],
        "comments": [{ "id": "1", "author": "Ana", "body": "oi", "at": "2024-01-01T00:00:00Z" }],
    });
    let shown = issue(&raw);
    assert_eq!(shown["subtasks"][0]["done"], true);
    assert_eq!(shown["attachments"][0]["size"], "10");
    assert_eq!(shown["comments"][0]["author"]["name"], "Ana");
    assert_eq!(shown["assignee"], Value::Null);
}

#[test]
fn a_change_and_a_pipeline_take_the_page_s_shape() {
    let pull = json!({ "number": 3, "title": "PR", "url": "https://c/3", "state": "open", "head": "h", "base": "main",
        "reviews": [{ "id": 1, "user": "bia", "state": "APPROVED", "body": "", "at": "x" }] });
    let files =
        json!({ "files": [{ "path": "a.rs", "additions": 1, "deletions": 0, "patch": "@@ -1,2 +4,3 @@\n+x" }] });
    let threads = json!({ "threads": [{ "path": "a.rs", "line": 4, "comments": [{ "id": "c", "author": "bia", "body": "?", "at": "x" }] }] });
    let data = change_data("o/r#3", &pull, &files, &threads);
    assert_eq!(data["files"][0]["firstLine"], 4);
    assert_eq!(data["comments"][0]["kind"], "approved");
    assert_eq!(data["comments"][1]["line"], 4);
    let run = json!({ "name": "CI", "status": "completed", "conclusion": "failure", "jobs": [
        { "id": 1, "key": "build", "name": "build", "status": "completed", "conclusion": "success", "needs": [], "steps": [] },
        { "id": 2, "key": "test", "name": "test", "status": "completed", "conclusion": "failure", "needs": ["build"], "steps": [] },
    ] });
    let shown = pipeline_run("o/r#9", &run);
    assert_eq!(shown["state"], "failed");
    assert_eq!(shown["jobs"][1]["stage"], 1);
}
