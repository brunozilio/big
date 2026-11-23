use serde_json::Value;

use super::{Api, valid_repo};
use crate::error::ActionError;
use crate::provider::Args;

const RUNS: &str = "/runs/";

pub fn pull_ref(repo: &str, number: u64) -> String {
    format!("{repo}#{number}")
}

pub fn run_ref(repo: &str, run: u64) -> String {
    format!("{repo}{RUNS}{run}")
}

fn number_of(text: &str, what: &str) -> Result<u64, ActionError> {
    text.trim().parse::<u64>().map_err(|_| ActionError::permanent(format!("{what} {text:?} is not a number")))
}

fn given(args: Args<'_>, name: &str) -> Option<String> {
    match args.0.get(name)? {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

pub fn repo_of(api: &Api<'_>, args: Args<'_>) -> Result<String, ActionError> {
    let repo = args.optional_text("repo").map(str::to_owned).or_else(|| api.default_repo());
    valid_repo(&repo.ok_or_else(|| ActionError::missing("repo"))?)
}

fn split(api: &Api<'_>, args: Args<'_>, text: &str, mark: &str, what: &str) -> Result<(String, u64), ActionError> {
    match text.rsplit_once(mark) {
        Some((repo, number)) => Ok((valid_repo(repo)?, number_of(number, what)?)),
        None => Ok((repo_of(api, args)?, number_of(text, what)?)),
    }
}

pub fn pull_of(api: &Api<'_>, args: Args<'_>) -> Result<(String, u64), ActionError> {
    match given(args, "pr") {
        Some(text) => split(api, args, &text, "#", "the pull request"),
        None => Ok((repo_of(api, args)?, args.number("number")?)),
    }
}

pub fn run_of(api: &Api<'_>, args: Args<'_>) -> Result<(String, u64), ActionError> {
    let text = given(args, "run").ok_or_else(|| ActionError::missing("run"))?;
    split(api, args, &text, RUNS, "the run")
}

#[cfg(test)]
mod tests {
    use domain::connection::ConnectionId;
    use serde_json::json;

    use super::*;
    use crate::http::FakeHttp;
    use crate::provider::Credentials;

    #[test]
    fn refs_name_the_repository_and_fall_back_to_the_connection_s_repository() {
        let creds = Credentials::new(&ConnectionId::from("c"), "t", json!({ "repos": ["acme/app"] }));
        let http = FakeHttp::new();
        let api = Api::new(&http, &creds);
        let pr = json!({ "pr": "o/r#12" });
        assert_eq!(pull_of(&api, Args(&pr)).ok(), Some(("o/r".to_owned(), 12)));
        let bare = json!({ "pr": 7 });
        assert_eq!(pull_of(&api, Args(&bare)).ok(), Some(("acme/app".to_owned(), 7)));
        let run = json!({ "run": run_ref("o/r", 9) });
        assert_eq!(run_of(&api, Args(&run)).ok(), Some(("o/r".to_owned(), 9)));
        assert!(pull_of(&api, Args(&json!({ "pr": "../x#1" }))).is_err());
        assert!(run_of(&api, Args(&json!({ "run": "o/r/runs/x" }))).is_err());
    }
}
