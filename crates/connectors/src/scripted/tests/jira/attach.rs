use serde_json::json;

use super::{call, session};
use crate::http::Method;

#[tokio::test]
async fn evidence_files_are_uploaded_once_to_the_issue_through_the_rest_api() {
    let issue = json!({ "id": 10001, "key": "BIG-1", "fields": {
        "summary": "T", "status": {"name": "Fazendo"}, "attachment": [{"filename": "a.png", "size": 3}],
    }});
    let http = session(&[issue]);
    let files = json!([
        { "name": "a.png", "contentType": "image/png", "base64": "cG5n" },
        { "name": "b.png", "contentType": "image/png", "base64": "cG5n" },
        { "name": "c.webm", "size": 99, "skipped": "too large" },
    ]);
    let answer = call(&http, "tracker.attach", json!({ "issue": "BIG-1", "files": files }), "k1").await;
    assert_eq!(answer, Ok(json!({ "attached": ["a.png", "b.png"], "skipped": ["c.webm: too large"] })));
    let uploads: Vec<_> = http.sent().into_iter().filter(|request| request.url.contains("/attachments")).collect();
    assert_eq!(uploads.len(), 1, "a.png is already on the issue");
    let upload = &uploads[0];
    assert_eq!(upload.method, Method::Post);
    assert_eq!(upload.url, "https://api.atlassian.com/ex/jira/cloud1/rest/api/3/issue/BIG-1/attachments");
    assert_eq!(upload.header_value("x-atlassian-token"), Some("no-check"));
    assert_eq!(upload.header_value("authorization"), Some("Bearer at"));
    let body = String::from_utf8_lossy(&upload.body).into_owned();
    assert!(body.contains("name=\"file\"; filename=\"b.png\"") && body.contains("\r\n\r\npng\r\n"), "{body}");
}
