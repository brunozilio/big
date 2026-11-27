use std::io::{BufRead, Write};
use std::process::ExitCode;
use std::time::Duration;

use crate::http::{exchange, place};

const WAIT: Duration = Duration::from_secs(60);

#[derive(Debug, clap::Args)]
pub struct Args {
    url: String,
    operation: String,
}

fn encoded(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => char::from(byte).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

pub fn asked(lines: impl Iterator<Item = String>) -> String {
    let mut fields = std::collections::HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.to_owned(), value.to_owned());
        }
    }
    let protocol = fields.get("protocol").map_or("https", String::as_str);
    let host = fields.get("host").map_or("", String::as_str);
    let path = fields.get("path").map_or(String::new(), |path| format!("/{path}"));
    format!("{protocol}://{host}{path}")
}

pub fn run(args: Args) -> Result<ExitCode, String> {
    if args.operation != "get" {
        return Ok(ExitCode::SUCCESS);
    }
    let stdin = std::io::stdin();
    let lines = stdin.lock().lines().map_while(Result::ok).take_while(|line| !line.is_empty());
    let wanted = asked(lines);
    let url = format!("{}?url={}", args.url, encoded(&wanted));
    let reply = exchange(&place(&url)?, "GET", &[], "", WAIT)?;
    if reply.code != 200 {
        return Ok(ExitCode::SUCCESS);
    }
    let mut out = std::io::stdout().lock();
    out.write_all(reply.body.as_bytes()).map_err(|error| error.to_string())?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_git_asks_becomes_an_address() {
        let lines = ["protocol=https", "host=github.com", "path=acme/loja.git"].map(str::to_owned);
        assert_eq!(asked(lines.into_iter()), "https://github.com/acme/loja.git");
        assert_eq!(encoded("https://a/b c"), "https%3A%2F%2Fa%2Fb%20c");
    }
}
