use serde_json::{Value, json};

use super::{Reach, text};
use crate::model::http::{Http, reason};
use crate::model::{CancellationToken, ModelError};
use crate::tools::Workspace;
use crate::tools::builtin::web::{reach as guard, under};

pub const SPACE: &str = "http";

const READ: usize = 5 << 20;
const SEARCH_READ: usize = 2 << 20;
const SEARCH_OVER_HTTP: &str = "https://html.duckduckgo.com/html/";
const SEARCH_HOST: &str = "html.duckduckgo.com";

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "fetch" => fetch(reach, arguments),
        "search" => search(reach, arguments),
        "reading" => Ok(reading(reach.workspace)),
        "request" | "stream.open" | "stream.next" | "stream.close" => scripted(reach, name, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn flags(web: domain::web_reading::WebReading) -> Value {
    json!({
        "mainContent": web.main_content,
        "cleanup": web.cleanup,
        "links": web.links,
        "smallPagesDirect": web.small_pages_direct,
        "formats": web.formats,
        "emptyPageError": web.empty_page_error,
    })
}

fn reading(workspace: &Workspace) -> Value {
    let mut read = flags(workspace.policy.web);
    read["configured"] = flags(domain::web_reading::WebReading::configured());
    read
}

fn fetch(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let url = guard::secure(text(arguments, "url")?);
    reach.fence.reaches(&domain::site::site(&url).unwrap_or_default())?;
    let Some(web) = &reach.workspace.services.network else {
        return Err(guard::refusal_by_name(&url).unwrap_or_else(|| "The web is not available to this run.".into()));
    };
    let mut headers = vec![("User-Agent".to_owned(), brand::web::USER_AGENT.to_owned())];
    if let Some(accept) = arguments.get("accept").and_then(Value::as_str) {
        headers.push(("Accept".to_owned(), accept.to_owned()));
    }
    let cancel = reach.workspace.stop.token();
    under(cancel, fetched(&*web.0, &url, &headers, cancel))
}

async fn fetched(
    web: &dyn Http,
    url: &str,
    headers: &[(String, String)],
    cancel: &CancellationToken,
) -> Result<Value, String> {
    let guard::Reached { url, response } = guard::fetch(web, url, headers, cancel).await?;
    let (status, content_type) = (response.status, response.header("content-type"));
    let read = response.body.capped(cancel, READ).await;
    let bytes = match read {
        Ok(bytes) => bytes,
        Err(_) if !(200..300).contains(&status) => Vec::new(),
        Err(error) => return Err(format!("Could not read {url}: {}", reason(error))),
    };
    Ok(json!({
        "status": status,
        "url": url,
        "contentType": content_type,
        "body": String::from_utf8_lossy(&bytes),
    }))
}

fn scripted(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    let streams = reach.streams.ok_or_else(|| format!("ctx.{SPACE}.{name} is not available here"))?;
    let mut arguments = arguments.clone();
    if let Some(url) = arguments.get("url").and_then(Value::as_str) {
        let url = guard::secure(url);
        reach.fence.reaches(&domain::site::site(&url).unwrap_or_default())?;
        arguments["url"] = Value::String(url);
    }
    let Some(web) = &reach.workspace.services.network else {
        return Err("The web is not available to this run.".into());
    };
    let cancel = reach.workspace.stop.token();
    under(cancel, streams.call(&*web.0, name, &arguments, cancel))
}

fn search(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let query = text(arguments, "query")?;
    reach.fence.reaches(SEARCH_HOST)?;
    let cancel = reach.workspace.stop.token();
    under(cancel, searched(query, reach.workspace, cancel))
}

async fn searched(query: &str, workspace: &Workspace, cancel: &CancellationToken) -> Result<Value, String> {
    let account = match &workspace.services.search {
        Some(search) => search.0.search(query, cancel).await,
        None => None,
    };
    let failed = match account {
        Some(Ok(hits)) if !hits.is_empty() => return Ok(json!({ "hits": hits })),
        Some(Ok(_)) | None => None,
        Some(Err(ModelError::Cancelled)) => return Err(ModelError::Cancelled.to_string()),
        Some(Err(error)) => {
            tracing::warn!(%error, "the account's web search failed; searching over HTTP");
            Some(error.to_string())
        }
    };
    match over_http(query, workspace, cancel).await {
        Ok(page) => Ok(json!({ "page": page })),
        Err(why) => Err(match failed {
            Some(first) => format!("The web search failed: {first}; then over HTTP: {why}"),
            None => why,
        }),
    }
}

async fn over_http(query: &str, workspace: &Workspace, cancel: &CancellationToken) -> Result<String, String> {
    let Some(web) = &workspace.services.network else {
        return Err("The web is not available to this run.".into());
    };
    let address = format!(
        "{SEARCH_OVER_HTTP}?{}",
        url::form_urlencoded::Serializer::new(String::new()).append_pair("q", query).finish()
    );
    let headers = [("User-Agent".to_owned(), brand::web::USER_AGENT.to_owned())];
    let response = web
        .0
        .request_cancellable(&address, &headers, None, cancel)
        .await
        .map_err(|error| format!("Could not search the web: {}", reason(error)))?;
    if !(200..300).contains(&response.status) {
        return Err(format!("The web search answered {}", response.status));
    }
    let bytes = response
        .body
        .capped(cancel, SEARCH_READ)
        .await
        .map_err(|error| format!("Could not read the web search: {}", reason(error)))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
