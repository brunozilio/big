use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[derive(Debug, PartialEq, Eq)]
pub struct Place {
    pub host: String,
    pub path: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reply {
    pub status: String,
    pub code: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }
}

pub fn place(url: &str) -> Result<Place, String> {
    let rest = url.strip_prefix("http://").ok_or_else(|| format!("{url} is not an http:// address"))?;
    let (host, path) = rest.split_once('/').map_or((rest, "/".to_owned()), |(host, path)| (host, format!("/{path}")));
    let local = host.starts_with("127.0.0.1:") || host.starts_with("localhost:") || host.starts_with("[::1]:");
    if !local {
        return Err(format!("{url} is not on this computer"));
    }
    Ok(Place { host: host.to_owned(), path })
}

pub fn parse(response: &[u8]) -> Result<Reply, String> {
    let text = String::from_utf8_lossy(response);
    let (head, body) = text.split_once("\r\n\r\n").ok_or("the answer has no body")?;
    let mut lines = head.lines();
    let status = lines.next().unwrap_or_default().to_owned();
    let code = status.split_whitespace().nth(1).and_then(|code| code.parse().ok()).unwrap_or(0);
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let mut reply = Reply { status, code, headers, body: String::new() };
    let chunked = reply.header("transfer-encoding").is_some_and(|value| value.to_ascii_lowercase().contains("chunked"));
    reply.body = if chunked { unchunked(body)? } else { body.to_owned() };
    Ok(reply)
}

fn unchunked(mut rest: &str) -> Result<String, String> {
    let mut body = String::new();
    loop {
        let (size, after) = rest.split_once("\r\n").ok_or("a chunk has no size")?;
        let size = usize::from_str_radix(size.split(';').next().unwrap_or_default().trim(), 16)
            .map_err(|error| format!("a chunk size is wrong: {error}"))?;
        if size == 0 {
            return Ok(body);
        }
        let chunk = after.get(..size).ok_or("a chunk is cut short")?;
        body.push_str(chunk);
        rest = after.get(size..).unwrap_or_default().trim_start_matches("\r\n");
    }
}

pub fn exchange(
    place: &Place,
    method: &str,
    headers: &[(&str, &str)],
    body: &str,
    wait: Duration,
) -> Result<Reply, String> {
    let mut stream = TcpStream::connect(&place.host).map_err(|error| format!("{}: {error}", place.host))?;
    stream.set_read_timeout(Some(wait)).map_err(|error| error.to_string())?;
    let extra: String = headers.iter().map(|(key, value)| format!("{key}: {value}\r\n")).collect();
    let request = format!(
        "{method} {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n{extra}Content-Length: {}\r\n\r\n{body}",
        place.path,
        place.host,
        body.len()
    );
    stream.write_all(request.as_bytes()).map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(|error| error.to_string())?;
    parse(&response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_local_http_address_is_asked() {
        let found = place("http://127.0.0.1:4100/api/cloud/credentials/abc").expect("place");
        assert_eq!(found, Place { host: "127.0.0.1:4100".into(), path: "/api/cloud/credentials/abc".into() });
        assert!(place("https://127.0.0.1:4100/x").is_err());
        assert!(place("http://example.com:80/x").is_err());
    }

    #[test]
    fn a_plain_answer_gives_its_status_headers_and_body() {
        let answer = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nMcp-Session-Id: s1\r\ncontent-length: 13\r\n\r\n{\"Version\":1}";
        let reply = parse(answer).expect("reply");
        assert_eq!((reply.code, reply.body.as_str()), (200, "{\"Version\":1}"));
        assert_eq!(reply.header("mcp-session-id"), Some("s1"));
    }

    #[test]
    fn a_chunked_answer_is_joined() {
        let answer = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{\"Ver\r\n8\r\nsion\":1}\r\n0\r\n\r\n";
        assert_eq!(parse(answer).expect("reply").body, "{\"Version\":1}");
    }
}
