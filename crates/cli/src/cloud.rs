use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

use crate::http::{Reply, exchange, place};

const WAIT: Duration = Duration::from_secs(60);

#[derive(Debug, clap::Args)]
pub struct Args {
    url: String,
}

fn body_of(reply: &Reply) -> Result<String, String> {
    if reply.code != 200 {
        return Err(format!("{}: {}", reply.status, reply.body.trim()));
    }
    Ok(reply.body.clone())
}

pub fn run(args: Args) -> Result<ExitCode, String> {
    let body = body_of(&exchange(&place(&args.url)?, "GET", &[], "", WAIT)?)?;
    let mut out = std::io::stdout().lock();
    out.write_all(body.trim().as_bytes()).map_err(|error| error.to_string())?;
    out.write_all(b"\n").map_err(|error| error.to_string())?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::parse;

    #[test]
    fn an_error_answer_fails_with_its_status() {
        let answer = b"HTTP/1.1 404 Not Found\r\ncontent-length: 2\r\n\r\n{}";
        let error = body_of(&parse(answer).expect("reply")).expect_err("not found");
        assert!(error.starts_with("HTTP/1.1 404 Not Found"), "{error}");
    }

    #[test]
    fn a_good_answer_gives_its_body() {
        let answer = b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}";
        assert_eq!(body_of(&parse(answer).expect("reply")).expect("body"), "{}");
    }
}
