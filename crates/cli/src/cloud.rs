use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::ExitCode;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(60);

#[derive(Debug, clap::Args)]
pub struct Args {
    url: String,
}

#[derive(Debug, PartialEq, Eq)]
struct Place {
    host: String,
    path: String,
}

fn place(url: &str) -> Result<Place, String> {
    let rest = url.strip_prefix("http://").ok_or_else(|| format!("{url} is not an http:// address"))?;
    let (host, path) = rest.split_once('/').map_or((rest, "/".to_owned()), |(host, path)| (host, format!("/{path}")));
    let local = host.starts_with("127.0.0.1:") || host.starts_with("localhost:") || host.starts_with("[::1]:");
    if !local {
        return Err(format!("{url} is not on this computer"));
    }
    Ok(Place { host: host.to_owned(), path })
}

fn body_of(response: &[u8]) -> Result<String, String> {
    let text = String::from_utf8_lossy(response);
    let (head, body) = text.split_once("\r\n\r\n").ok_or("the answer has no body")?;
    let status = head.lines().next().unwrap_or_default();
    let code = status.split_whitespace().nth(1).unwrap_or_default();
    let chunked = head.lines().any(|line| {
        let line = line.to_ascii_lowercase();
        line.starts_with("transfer-encoding:") && line.contains("chunked")
    });
    let body = if chunked { unchunked(body)? } else { body.to_owned() };
    if code != "200" {
        return Err(format!("{status}: {}", body.trim()));
    }
    Ok(body)
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

fn fetch(place: &Place) -> Result<Vec<u8>, String> {
    let mut stream = TcpStream::connect(&place.host).map_err(|error| format!("{}: {error}", place.host))?;
    stream.set_read_timeout(Some(WAIT)).map_err(|error| error.to_string())?;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
        place.path, place.host
    );
    stream.write_all(request.as_bytes()).map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(|error| error.to_string())?;
    Ok(response)
}

pub fn run(args: Args) -> Result<ExitCode, String> {
    let body = body_of(&fetch(&place(&args.url)?)?)?;
    let mut out = std::io::stdout().lock();
    out.write_all(body.trim().as_bytes()).map_err(|error| error.to_string())?;
    out.write_all(b"\n").map_err(|error| error.to_string())?;
    Ok(ExitCode::SUCCESS)
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
    fn a_plain_answer_gives_its_body() {
        let answer = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 13\r\n\r\n{\"Version\":1}";
        assert_eq!(body_of(answer).expect("body"), "{\"Version\":1}");
    }

    #[test]
    fn a_chunked_answer_is_joined() {
        let answer = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{\"Ver\r\n8\r\nsion\":1}\r\n0\r\n\r\n";
        assert_eq!(body_of(answer).expect("body"), "{\"Version\":1}");
    }

    #[test]
    fn an_error_answer_fails_with_its_status() {
        let answer = b"HTTP/1.1 404 Not Found\r\ncontent-length: 2\r\n\r\n{}";
        let error = body_of(answer).expect_err("not found");
        assert!(error.starts_with("HTTP/1.1 404 Not Found"), "{error}");
    }
}
