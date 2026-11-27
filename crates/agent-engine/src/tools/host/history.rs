use serde_json::Value;

use super::Reach;
use crate::history::Query;

pub const SPACE: &str = "history";

const LISTED: usize = 20;
const LISTED_AT_MOST: usize = 100;
const READ: usize = 20;

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    let history = reach.workspace.services.history.as_ref().ok_or("this run keeps no history to look into")?;
    let history = &history.0;
    match name {
        "places" => Ok(shown(&history.places())),
        "sessions" => Ok(shown(&history.sessions(&query(arguments)))),
        "search" => {
            let query = query(arguments);
            if query.text.trim().is_empty() {
                return Err("say what to search for in `text`".into());
            }
            Ok(shown(&history.search(&query)))
        }
        "read" => {
            let session = arguments.get("session").and_then(Value::as_str).ok_or("session must be a session id")?;
            let from = whole(arguments, "from").unwrap_or(0);
            let count = whole(arguments, "count").unwrap_or(READ);
            history.read(session, from, count).map(|read| shown(&read))
        }
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn whole(arguments: &Value, key: &str) -> Option<usize> {
    arguments.get(key).and_then(Value::as_u64).and_then(|value| usize::try_from(value).ok())
}

fn query(arguments: &Value) -> Query {
    let text = |key: &str| arguments.get(key).and_then(Value::as_str).map(str::to_owned);
    Query {
        text: text("text").unwrap_or_default(),
        cwd: text("cwd"),
        since_ms: arguments.get("since_ms").and_then(Value::as_u64),
        limit: whole(arguments, "limit").unwrap_or(LISTED).clamp(1, LISTED_AT_MOST),
    }
}

fn shown(value: &impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::super::Fence;
    use super::*;
    use crate::history::{History, HostHistory, Place, Read, Session};
    use crate::tools::Workspace;

    struct Kept;

    fn listed() -> Session {
        Session { session: "abc".into(), kept_in: "current".into(), title: None, cwd: None, updated_ms: 1 }
    }

    impl History for Kept {
        fn places(&self) -> Vec<Place> {
            vec![Place::new("sessions", "/nowhere-at-all".into(), "conversations")]
        }
        fn sessions(&self, query: &Query) -> Vec<Session> {
            vec![Session {
                title: Some(format!("{}:{}", query.limit, query.cwd.clone().unwrap_or_default())),
                ..listed()
            }]
        }
        fn search(&self, _query: &Query) -> Vec<crate::history::Hit> {
            Vec::new()
        }
        fn read(&self, session: &str, from: usize, count: usize) -> Result<Read, String> {
            Ok(Read {
                session: Session { title: Some(format!("{session}:{from}:{count}")), ..listed() },
                total: 0,
                turns: Vec::new(),
            })
        }
    }

    fn asked(workspace: &Workspace, name: &str, arguments: Value) -> Result<Value, String> {
        call(&Reach { workspace, fence: Fence::of(&[]), streams: None }, name, &arguments)
    }

    #[test]
    fn the_history_primitives_reach_what_the_host_keeps() {
        let mut workspace = Workspace::new(std::env::temp_dir());
        assert!(asked(&workspace, "places", Value::Null).is_err(), "no history without a host that keeps one");
        workspace.services.history = Some(HostHistory(Arc::new(Kept)));
        let places = asked(&workspace, "places", Value::Null).expect("places");
        assert_eq!(places[0]["name"], "sessions");
        assert_eq!(places[0]["exists"], false);
        let listed = asked(&workspace, "sessions", json!({"limit": 500, "cwd": "/w"})).expect("sessions");
        assert_eq!(listed[0]["title"], "100:/w", "the limit is capped");
        let read = asked(&workspace, "read", json!({"session": "abcdefgh", "from": 3})).expect("read");
        assert_eq!(read["title"], "abcdefgh:3:20");
        assert!(asked(&workspace, "search", json!({"text": "  "})).is_err());
        assert!(asked(&workspace, "forget", Value::Null).is_err());
    }
}
