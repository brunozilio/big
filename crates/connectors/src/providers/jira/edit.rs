use serde_json::{Map, Value, json};

use super::tracker::{read, summary};
use super::{Api, jql_key};
use crate::error::ActionError;
use crate::provider::{Args, text_of};

const DEFAULT_LINK: &str = "Relates";

pub async fn update_issue(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let issue = jql_key(args.text("issue")?)?;
    let mut fields = match args.0.get("fields") {
        Some(Value::Object(given)) => given.clone(),
        _ => Map::new(),
    };
    if let Some(title) = args.optional_text("title") {
        fields.insert("summary".to_owned(), json!(title));
    }
    if let Some(description) = args.0.get("description").and_then(Value::as_str) {
        fields.insert("description".to_owned(), json!(description));
    }
    if fields.is_empty() {
        return Err(ActionError::missing("fields"));
    }
    let arguments = json!({ "issueIdOrKey": issue, "fields": fields, "contentFormat": "markdown" });
    let answer = api.tool("editJiraIssue", arguments).await?;
    let mut made = summary(api, &answer);
    made["ref"] = json!(issue);
    made["changed"] = json!(true);
    Ok(made)
}

fn linked(links: &Value, other: &str) -> bool {
    links.as_array().into_iter().flatten().any(|link| {
        let inward = text_of(link, "/inwardIssue/key");
        let outward = text_of(link, "/outwardIssue/key");
        inward.eq_ignore_ascii_case(other) || outward.eq_ignore_ascii_case(other)
    })
}

pub async fn link_issues(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let issue = jql_key(args.text("issue")?)?;
    let to = jql_key(args.text("to")?)?;
    let current = read(api, issue, &["issuelinks"]).await?;
    let links = current.pointer("/fields/issuelinks").cloned().unwrap_or(Value::Null);
    if linked(&links, to) {
        return Ok(json!({ "issue": issue, "to": to, "already": true }));
    }
    let kind = args.optional_text("kind").unwrap_or(DEFAULT_LINK);
    let arguments = json!({ "inwardIssue": to, "outwardIssue": issue, "type": kind });
    api.tool("createIssueLink", arguments).await?;
    Ok(json!({ "issue": issue, "to": to, "already": false }))
}
