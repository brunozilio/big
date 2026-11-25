use serde_json::{Value, json};

use super::Api;
use super::refs::repo_of;
use crate::error::ActionError;
use crate::http::Method;
use crate::provider::{Args, text_of};

fn branch(args: Args<'_>, name: &str) -> Result<String, ActionError> {
    let value = args.text(name)?;
    let valid = !value.starts_with('-') && !value.contains("..") && !value.chars().any(char::is_whitespace);
    if valid { Ok(value.to_owned()) } else { Err(ActionError::permanent(format!("{value:?} is not a branch"))) }
}

pub async fn merge_branches(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let repo = repo_of(api, args)?;
    let base = branch(args, "base")?;
    let head = branch(args, "head")?;
    let message = args.optional_text("message").map_or_else(|| format!("Merge {head} into {base}"), str::to_owned);
    let body = json!({ "base": base, "head": head, "commit_message": message });
    let done = api.send(Method::Post, &format!("/repos/{repo}/merges"), &body).await?;
    if done.is_null() {
        return Ok(json!({ "merged": true, "already": true }));
    }
    Ok(json!({ "merged": true, "already": false, "sha": text_of(&done, "/sha") }))
}
