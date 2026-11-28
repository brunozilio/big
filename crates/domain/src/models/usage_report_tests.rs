use super::*;

const KNOWN: [&str; 2] = ["claude-subscription", "openai-subscription"];

fn window() -> Vec<String> {
    ["2026-11-21", "2026-11-22", "2026-11-23"].iter().map(|date| (*date).to_owned()).collect()
}

fn spent(provider: &str, account: &str, model: &str, cost: Option<f64>) -> EngineEvent {
    EngineEvent::Usage(ModelUsage {
        model: model.into(),
        input: 100,
        output: 10,
        cache_read: 1000,
        cache_write: 5,
        estimated_cost_usd: cost,
        provider: provider.into(),
        account: account.into(),
        ..ModelUsage::default()
    })
}

#[test]
fn every_day_of_the_window_is_listed_even_without_use() {
    let report = UsageTally::new(&window(), &KNOWN).report();
    let dates: Vec<&str> = report.daily.iter().map(|day| day.date.as_str()).collect();
    assert_eq!(dates, ["2026-11-21", "2026-11-22", "2026-11-23"]);
    assert!(report.daily.iter().all(|day| day.tally == Tally::default() && day.by_provider.is_empty()));
    assert_eq!(report.totals, Totals::default());
}

#[test]
fn use_outside_the_window_is_left_out() {
    let mut tally = UsageTally::new(&window(), &KNOWN);
    tally.add("s1", "2026-11-20", &spent("claude-subscription", "a", "opus", Some(1.0)));
    tally.add("s1", "2026-11-22", &spent("claude-subscription", "a", "opus", Some(0.5)));
    let report = tally.report();
    assert_eq!(report.totals.cost.known_micros, 500_000);
    assert_eq!(report.totals.calls, 1);
    assert_eq!(report.daily[1].tally.calls, 1);
    assert_eq!(report.daily[1].tally.tokens, 1115);
}

#[test]
fn use_is_ranked_by_the_agent_that_ran_the_session_and_a_plain_chat_has_none() {
    let agents = [("s1".to_owned(), "dev".to_owned()), ("s2".to_owned(), "planner".to_owned())].into();
    let mut tally = UsageTally::new(&window(), &KNOWN).with_agents(agents);
    tally.add("s1", "2026-11-22", &spent("claude-subscription", "a", "opus", Some(1.0)));
    tally.add("s1", "2026-11-23", &spent("claude-subscription", "a", "opus", Some(0.5)));
    tally.add("s2", "2026-11-23", &spent("claude-subscription", "a", "opus", Some(0.25)));
    tally.add("chat", "2026-11-23", &spent("claude-subscription", "a", "opus", Some(4.0)));
    let report = tally.report();
    let ranked: Vec<(&str, u64)> =
        report.by_agent.iter().map(|ranked| (ranked.id.as_str(), ranked.tally.calls)).collect();
    assert_eq!(ranked, [("dev", 2), ("planner", 1)]);
    assert_eq!(report.by_agent[0].tally.cost.known_micros, 1_500_000);
}

#[test]
fn a_call_without_a_price_is_counted_apart_and_the_known_part_stays_a_partial() {
    let mut tally = UsageTally::new(&window(), &KNOWN);
    tally.add("s1", "2026-11-23", &spent("claude-subscription", "a", "opus", Some(0.5)));
    tally.add("s1", "2026-11-23", &spent("claude-subscription", "a", "opus", Some(0.25)));
    tally.add("s1", "2026-11-23", &spent("claude-subscription", "a", "local", None));
    let report = tally.report();
    assert_eq!((report.totals.cost.known_micros, report.totals.cost.unpriced_calls), (750_000, 1));
    assert_eq!(report.daily[2].tally.cost.unpriced_calls, 1);
    let local = report.by_model.iter().find(|ranked| ranked.id == "local").expect("local");
    assert_eq!((local.tally.cost.known_micros, local.tally.cost.unpriced_calls), (0, 1));
}

#[test]
fn a_sub_agent_s_use_counts_too() {
    let mut tally = UsageTally::new(&window(), &KNOWN);
    let inner = spent("claude-subscription", "a", "haiku", Some(0.25));
    let nested = EngineEvent::AgentEvent {
        index: 0,
        event: Box::new(EngineEvent::AgentEvent { index: 1, event: Box::new(inner) }),
    };
    tally.add("s1", "2026-11-23", &nested);
    tally.add("s1", "2026-11-23", &EngineEvent::TextDelta("hi".into()));
    let report = tally.report();
    assert_eq!(report.totals.calls, 1);
    assert_eq!(report.by_model[0].id, "haiku");
}

#[test]
fn rankings_go_from_the_most_to_the_least_expensive() {
    let mut tally = UsageTally::new(&window(), &KNOWN);
    tally.add("s1", "2026-11-21", &spent("openai-subscription", "y", "gpt-5", Some(0.1)));
    tally.add("s2", "2026-11-22", &spent("claude-subscription", "a", "opus", Some(2.0)));
    tally.add("s2", "2026-11-23", &spent("claude-subscription", "b", "opus", Some(1.0)));
    tally.add("s3", "2026-11-23", &spent("openai-subscription", "y", "gpt-5", None));
    let report = tally.report();
    let providers: Vec<(&str, u64)> = report.by_provider.iter().map(|r| (r.id.as_str(), r.tally.calls)).collect();
    assert_eq!(providers, [("claude-subscription", 2), ("openai-subscription", 2)]);
    let models: Vec<&str> = report.by_model.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(models, ["opus", "gpt-5"]);
    let accounts: Vec<&str> = report.by_account.iter().map(|r| r.account.as_str()).collect();
    assert_eq!(accounts, ["a", "b", "y"]);
    assert_eq!(report.totals.sessions, 3);
    assert_eq!(
        (report.totals.input, report.totals.output, report.totals.cache_read, report.totals.cache_write),
        (400, 40, 4000, 20)
    );
    assert!((report.daily[2].by_provider["claude-subscription"] - 1.0).abs() < f64::EPSILON);
    assert!(report.daily[2].by_model["gpt-5"].abs() < f64::EPSILON);
}

#[test]
fn an_old_record_takes_its_provider_from_a_known_model_prefix_or_is_unknown() {
    let mut tally = UsageTally::new(&window(), &KNOWN);
    tally.add("s1", "2026-11-23", &spent("", "", "openai-subscription:gpt-5", Some(0.2)));
    tally.add("s1", "2026-11-23", &spent("", "", "mystery:model", Some(0.1)));
    tally.add("s1", "2026-11-23", &spent("", "", "opus", None));
    let report = tally.report();
    let providers: Vec<&str> = report.by_provider.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(providers, ["openai-subscription", UNKNOWN]);
    let models: Vec<&str> = report.by_model.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(models, ["gpt-5", "mystery:model", "opus"]);
    assert!(report.by_account.iter().all(|r| r.account.is_empty()));
}

#[test]
fn a_record_saved_before_provider_and_account_existed_still_reads() {
    let old = r#"{"model":"m","input":1,"output":2,"cache_read":0,"cache_write":0,"estimated_cost_usd":null,"basis":"","price_date":""}"#;
    let usage: ModelUsage = serde_json::from_str(old).expect("old record");
    assert_eq!((usage.provider.as_str(), usage.account.as_str()), ("", ""));
}

#[test]
fn use_is_split_by_origin_and_what_no_item_owns_is_counted_apart() {
    let agents = [("s1".to_owned(), "dev".to_owned())].into();
    let mut tally = UsageTally::new(&window(), &KNOWN).with_agents(agents);
    let charged = |origin, item: Option<&str>, cost| match spent("provider-a", "a", "model-x", Some(cost)) {
        EngineEvent::Usage(usage) => EngineEvent::Usage(ModelUsage {
            attribution: crate::origin::Attribution { item: item.map(str::to_owned), step: None, origin },
            ..usage
        }),
        other => other,
    };
    tally.add("s1", "2026-11-22", &charged(Origin::Step, None, 1.0));
    tally.add("s1", "2026-11-22", &charged(Origin::Summary, None, 0.25));
    tally.add("s9", "2026-11-22", &charged(Origin::Title, Some("i1"), 0.5));
    tally.add("chat", "2026-11-23", &charged(Origin::Step, None, 2.0));
    tally.add("chat", "2026-11-23", &charged(Origin::Filter, None, 0.125));
    let report = tally.report();
    let origins: Vec<(Origin, u64)> =
        report.by_origin.iter().map(|(origin, tally)| (*origin, tally.cost.known_micros)).collect();
    assert_eq!(
        origins,
        [(Origin::Step, 3_000_000), (Origin::Title, 500_000), (Origin::Summary, 250_000), (Origin::Filter, 125_000)]
    );
    assert_eq!((report.unattributed.calls, report.unattributed.cost.known_micros), (2, 2_125_000));
}
