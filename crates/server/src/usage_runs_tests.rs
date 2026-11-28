use std::path::PathBuf;

use super::*;
use domain::billing::ModelUsage;
use domain::conversation::SessionStatus;
use domain::models::usage_report::UsageTally;
use domain::{EngineEvent, SessionId};

fn listed(session: &str, title: Option<&str>) -> Listed {
    Listed {
        session: SessionId(session.to_owned()),
        cwd: PathBuf::from("/tmp"),
        title: title.map(str::to_owned),
        created: 0,
        updated: 0,
        status: SessionStatus::Done,
        unread: false,
    }
}

fn call(read: u64, cost: f64) -> ModelUsage {
    ModelUsage {
        model: "model-x".into(),
        input: 100,
        cache_read: read,
        estimated_cost_usd: Some(cost),
        provider: "provider-a".into(),
        ..ModelUsage::default()
    }
}

fn spawned() -> EngineEvent {
    EngineEvent::AgentSpawned {
        index: 2,
        kind: "explore".into(),
        summary: "find the parser".into(),
        prompt: String::new(),
    }
}

#[test]
fn each_run_lists_its_cache_hit_rate_and_the_cost_of_every_sub_agent() {
    let day = "2026-11-23".to_owned();
    let agents = [("s1".to_owned(), "dev".to_owned())].into();
    let mut tally = UsageTally::new(std::slice::from_ref(&day), &[]).with_agents(agents);
    tally.add("s1", &day, &EngineEvent::Usage(call(300, 1.0)));
    tally.add("s1", &day, &spawned());
    tally.add("s1", &day, &EngineEvent::AgentEvent { index: 2, event: Box::new(EngineEvent::Usage(call(0, 0.5))) });
    let rows = by_run(&tally.report().by_run, &[listed("s1", Some("Fix the parser"))]);
    let run = &rows[0];
    assert_eq!(
        (run["id"].as_str(), run["title"].as_str(), run["agent"].as_str()),
        (Some("s1"), Some("Fix the parser"), Some("dev"))
    );
    assert_eq!((run["costUsd"].as_f64(), run["calls"].as_u64()), (Some(1.5), Some(2)));
    assert_eq!(run["cacheHitRate"].as_f64(), Some(0.6));
    assert_eq!((run["lead"]["costUsd"].as_f64(), run["lead"]["cacheHitRate"].as_f64()), (Some(1.0), Some(0.75)));
    assert_eq!(
        run["subagents"],
        json!([{
            "index": 2, "kind": "explore", "summary": "find the parser",
            "costUsd": 0.5, "unpricedCalls": 0, "tokens": 100, "calls": 1, "cacheHitRate": 0.0,
        }])
    );
    assert_eq!((run["item"].clone(), run["step"].clone()), (Value::Null, Value::Null));
}

#[test]
fn a_run_without_a_listed_session_has_no_title() {
    let day = "2026-11-23".to_owned();
    let mut tally = UsageTally::new(std::slice::from_ref(&day), &[]);
    tally.add("gone", &day, &EngineEvent::Usage(call(0, 0.1)));
    let rows = by_run(&tally.report().by_run, &[]);
    assert_eq!((rows[0]["title"].clone(), rows[0]["agent"].clone()), (Value::Null, Value::Null));
    assert_eq!(rows[0]["subagents"], json!([]));
}
