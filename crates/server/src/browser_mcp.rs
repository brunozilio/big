use std::io::Write;
use std::path::{Path, PathBuf};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use domain::mcp::McpEndpoint;
use serde_json::{Value, json};

use crate::state::Ctx;

pub const SESSION_HEADER: &str = "mcp-session-id";
const EXTERNAL: &str = "external";

pub fn route() -> &'static str {
    brand::browser::MCP_ROUTE.trim()
}

pub fn token_file(data: &Path) -> PathBuf {
    data.join(brand::browser::MCP_TOKEN_FILE.trim())
}

pub fn token(data: &Path) -> std::io::Result<String> {
    let file = token_file(data);
    if let Ok(kept) = std::fs::read_to_string(&file) {
        let kept = kept.trim();
        if !kept.is_empty() {
            return Ok(kept.to_owned());
        }
    }
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
    let made: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    std::fs::create_dir_all(data)?;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(&file)?.write_all(made.as_bytes())?;
    Ok(made)
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .unwrap_or_default();
    !token.is_empty() && same(given.as_bytes(), token.as_bytes())
}

fn same(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && left.iter().zip(right).fold(0_u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

fn session_id(headers: &HeaderMap, body: &Value) -> Result<String, String> {
    let given = headers.get(SESSION_HEADER).and_then(|value| value.to_str().ok()).map(str::trim);
    match given {
        Some(id) if !id.is_empty() && id.len() <= 128 => Ok(id.to_owned()),
        _ if initializes(body) => crate::attachments::random_id().map_err(|_| "no random id".to_owned()),
        _ => Ok(EXTERNAL.to_owned()),
    }
}

fn initializes(body: &Value) -> bool {
    let one = |message: &Value| message["method"] == "initialize";
    body.as_array().map_or_else(|| one(body), |all| all.iter().any(one))
}

pub fn answer(endpoint: &dyn McpEndpoint, body: &Value) -> Option<Value> {
    match body.as_array() {
        Some(all) => {
            let replies: Vec<Value> = all.iter().filter_map(|message| endpoint.handle(message)).collect();
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        None => endpoint.handle(body),
    }
}

fn refused(status: StatusCode, message: &str) -> Response {
    let body = json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32000, "message": message } });
    (status, axum::Json(body)).into_response()
}

fn session(ctx: &Ctx, id: &str) -> browser::Session {
    let data = &ctx.backend.data;
    let settings = browser::Settings::configured(data);
    let roots = vec![ctx.backend.attachments(), settings.files.clone()];
    browser::Session::new(ctx.backend.browser.clone(), format!("{EXTERNAL}:{id}"), settings, roots)
}

pub async fn post(State(ctx): State<Ctx>, headers: HeaderMap, body: Bytes) -> Response {
    let token = token(&ctx.backend.data).unwrap_or_default();
    if !authorized(&headers, &token) {
        let file = token_file(&ctx.backend.data);
        let told = format!("Send Authorization: Bearer <token>; the token is in {}.", file.display());
        return refused(StatusCode::UNAUTHORIZED, &told);
    }
    if !crate::browsers::enabled(&ctx.backend.store) {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "The browser tools are turned off in the app.");
    }
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        return refused(StatusCode::BAD_REQUEST, "The body is not JSON-RPC.");
    };
    let id = match session_id(&headers, &message) {
        Ok(id) => id,
        Err(error) => return refused(StatusCode::INTERNAL_SERVER_ERROR, &error),
    };
    let endpoint = session(&ctx, &id);
    let replied = tokio::task::spawn_blocking(move || answer(&endpoint, &message)).await.ok().flatten();
    let mut response = match replied {
        Some(reply) => axum::Json(reply).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    };
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(SESSION_HEADER, value);
    }
    response
}

pub async fn other() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "POST, DELETE")]).into_response()
}

pub async fn ended() -> StatusCode {
    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    impl McpEndpoint for Echo {
        fn handle(&self, message: &Value) -> Option<Value> {
            message.get("id").map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": message["method"] }))
        }
    }

    fn bearer(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_str(value).unwrap_or(HeaderValue::from_static("")));
        headers
    }

    #[test]
    fn only_the_kept_token_opens_the_door() {
        assert!(authorized(&bearer("Bearer abc"), "abc"));
        assert!(!authorized(&bearer("Bearer abd"), "abc"));
        assert!(!authorized(&bearer("abc"), "abc"));
        assert!(!authorized(&HeaderMap::new(), "abc"));
        assert!(!authorized(&bearer("Bearer "), ""));
    }

    #[test]
    fn the_token_is_made_once_and_kept_private() {
        let data = tempfile::tempdir().expect("data");
        let first = token(data.path()).expect("made");
        assert_eq!(first.len(), 64);
        assert_eq!(token(data.path()).expect("kept"), first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(token_file(data.path())).expect("file").permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn notifications_get_no_reply_and_batches_reply_per_request() {
        assert!(answer(&Echo, &json!({ "method": "notifications/initialized" })).is_none());
        let one = answer(&Echo, &json!({ "id": 1, "method": "tools/list" })).expect("reply");
        assert_eq!(one["result"], "tools/list");
        let batch = json!([{ "id": 1, "method": "ping" }, { "method": "notifications/initialized" }]);
        let replies = answer(&Echo, &batch).expect("replies");
        assert_eq!(replies.as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn a_new_client_gets_its_own_session_and_keeps_it() {
        let initialize = json!({ "id": 1, "method": "initialize" });
        let made = session_id(&HeaderMap::new(), &initialize).expect("id");
        assert_ne!(made, EXTERNAL);
        let mut headers = HeaderMap::new();
        headers.insert(SESSION_HEADER, HeaderValue::from_static("kiro-1"));
        assert_eq!(session_id(&headers, &json!({ "id": 2, "method": "tools/list" })).expect("id"), "kiro-1");
        assert_eq!(session_id(&HeaderMap::new(), &json!({ "id": 2 })).expect("id"), EXTERNAL);
    }
}
