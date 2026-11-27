use axum::body::Body;
use axum::extract::Request;
use axum::http::header::{CONNECTION, CONTENT_LENGTH, HOST, ORIGIN, REFERER, SET_COOKIE, TRANSFER_ENCODING};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

const MOST_BODY: usize = 64 * 1024 * 1024;
const SKIPPED: [HeaderName; 6] = [HOST, ORIGIN, REFERER, CONNECTION, CONTENT_LENGTH, TRANSFER_ENCODING];

pub fn origin_of(remote: &str) -> String {
    let (scheme, rest) = remote.split_once("://").unwrap_or(("https", remote));
    let host = rest.split('/').next().unwrap_or(rest);
    format!("{scheme}://{host}")
}

pub fn local_cookie(cookie: &str) -> String {
    cookie
        .split(';')
        .map(str::trim)
        .filter(|part| {
            let lower = part.to_ascii_lowercase();
            lower != "secure" && !lower.starts_with("domain=")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build()
}

pub async fn forward(remote: &str, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path_and_query().map_or("/", |path| path.as_str());
    let Ok(bytes) = axum::body::to_bytes(body, MOST_BODY).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let Ok(client) = client() else { return StatusCode::BAD_GATEWAY.into_response() };
    let mut outgoing = client.request(parts.method.clone(), format!("{remote}{path}")).body(bytes);
    for (name, value) in &parts.headers {
        if !SKIPPED.contains(name) {
            outgoing = outgoing.header(name, value);
        }
    }
    outgoing = outgoing.header(ORIGIN, origin_of(remote));
    match outgoing.send().await {
        Ok(answer) => answered(answer),
        Err(error) => (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

fn answered(answer: reqwest::Response) -> Response {
    let status = answer.status();
    let mut response = Response::builder().status(status);
    for (name, value) in answer.headers() {
        if [CONNECTION, CONTENT_LENGTH, TRANSFER_ENCODING].contains(name) {
            continue;
        }
        if name == SET_COOKIE {
            if let Some(local) =
                value.to_str().ok().map(local_cookie).and_then(|text| HeaderValue::from_str(&text).ok())
            {
                response = response.header(SET_COOKIE, local);
            }
            continue;
        }
        response = response.header(name, value);
    }
    response.body(Body::from_stream(answer.bytes_stream())).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remote_cookie_is_kept_for_the_local_page_without_secure_or_domain() {
        assert_eq!(
            local_cookie("big_session=t; Path=/; HttpOnly; Secure; SameSite=Lax; Domain=x.example.com; Max-Age=9"),
            "big_session=t; Path=/; HttpOnly; SameSite=Lax; Max-Age=9"
        );
        assert_eq!(
            origin_of("https://abc.execute-api.us-east-1.amazonaws.com/app"),
            "https://abc.execute-api.us-east-1.amazonaws.com"
        );
    }
}
