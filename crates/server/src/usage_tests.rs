use super::*;
use domain::EngineEvent;
use domain::billing::ModelUsage;
use domain::provider::Access;

fn query(days: &str) -> HashMap<String, String> {
    HashMap::from([("days".to_owned(), days.to_owned())])
}

#[test]
fn only_a_week_a_month_or_a_quarter_is_offered_and_a_month_is_the_default() {
    assert_eq!(days_of(&query("7")), 7);
    assert_eq!(days_of(&query("90")), 90);
    assert_eq!(days_of(&query("12")), 30);
    assert_eq!(days_of(&query("muitos")), 30);
    assert_eq!(days_of(&HashMap::new()), 30);
}

#[test]
fn the_window_ends_today_and_crosses_months() {
    let today = jiff::civil::date(2026, 11, 2);
    assert_eq!(window(today, 3), ["2026-10-31", "2026-11-01", "2026-11-02"]);
    assert_eq!(window(today, 30).len(), 30);
}

#[test]
fn a_moment_falls_on_the_local_day() {
    let zone = TimeZone::fixed(jiff::tz::offset(-3));
    let utc_midnight_and_a_bit = 1_795_392_060_000;
    assert_eq!(day_of(utc_midnight_and_a_bit, &zone).as_deref(), Some("2026-11-22"));
    assert_eq!(day_of(utc_midnight_and_a_bit, &TimeZone::UTC).as_deref(), Some("2026-11-23"));
}

fn summary(id: &'static str, label: &str, logo: &str) -> ProviderSummary {
    ProviderSummary {
        provider: domain::ModelProvider::new(id),
        name: label.into(),
        label: label.into(),
        logo: logo.into(),
        access: Access::Code,
        account: None,
        accounts: Vec::new(),
        balance: Default::default(),
    }
}

#[test]
fn the_report_names_providers_and_counts_every_day() {
    let window = window(jiff::civil::date(2026, 11, 23), 7);
    let mut tally = UsageTally::new(&window, &["claude-subscription"]);
    let usage = |provider: &str, cost| {
        EngineEvent::Usage(ModelUsage {
            model: "claude-opus-4-5".into(),
            input: 10,
            output: 5,
            estimated_cost_usd: Some(cost),
            provider: provider.into(),
            account: "fulano@x.com".into(),
            ..ModelUsage::default()
        })
    };
    tally.add("s", "2026-11-23", &usage("claude-subscription", 0.5));
    tally.add("s", "2026-11-23", &usage("", 0.25));
    let providers = [summary("claude-subscription", "Claude", "claude")];
    let listed = listing(7, &window, &tally.report(), &providers, domain::Language::default());
    assert_eq!((listed["from"].as_str(), listed["to"].as_str()), (Some("2026-11-17"), Some("2026-11-23")));
    assert_eq!(listed["daily"].as_array().map(Vec::len), Some(7));
    assert_eq!(listed["daily"][0]["byProvider"], json!({}));
    assert_eq!(listed["daily"][6]["tokens"], 30);
    assert_eq!(listed["totals"]["sessions"], 1);
    assert_eq!(
        listed["byProvider"][0],
        json!({ "id": "claude-subscription", "name": "Claude", "logo": "claude", "costUsd": 0.5, "unpricedCalls": 0, "tokens": 15, "calls": 1, "cacheHitRate": 0.0 })
    );
    assert_eq!(listed["byProvider"][1]["name"], UNRECORDED);
    assert_eq!(listed["byAccount"][0]["id"], "claude-subscription:fulano@x.com");
    assert_eq!(listed["byModel"][0]["name"], "claude-opus-4-5");
}

#[test]
fn the_report_shows_the_known_part_and_how_many_calls_had_no_price() {
    let window = window(jiff::civil::date(2026, 11, 23), 7);
    let mut tally = UsageTally::new(&window, &["claude-subscription"]);
    let call = |cost| {
        EngineEvent::Usage(ModelUsage {
            model: "claude-opus-4-5".into(),
            estimated_cost_usd: cost,
            provider: "claude-subscription".into(),
            basis: "subscription".into(),
            ..ModelUsage::default()
        })
    };
    for cost in [Some(0.5), Some(0.25), None] {
        tally.add("s", "2026-11-23", &call(cost));
    }
    let listed = listing(7, &window, &tally.report(), &[], domain::Language::default());
    assert_eq!(listed["totals"]["costUsd"], 0.75);
    assert_eq!(listed["totals"]["unpricedCalls"], 1);
    assert_eq!(listed["totals"]["basis"], "subscription");
    assert_eq!(listed["daily"][6]["unpricedCalls"], 1);
    assert_eq!(listed["byModel"][0]["unpricedCalls"], 1);
}

#[test]
fn the_report_splits_the_spend_by_origin_and_counts_what_no_item_owns() {
    use domain::origin::{Attribution, Origin};
    let window = window(jiff::civil::date(2026, 11, 23), 7);
    let mut tally = UsageTally::new(&window, &["provider-a"]);
    let call = |cost, attribution| {
        EngineEvent::Usage(ModelUsage {
            model: "model-x".into(),
            estimated_cost_usd: Some(cost),
            provider: "provider-a".into(),
            attribution,
            ..ModelUsage::default()
        })
    };
    tally.add("s1", "2026-11-23", &call(1.0, Attribution::of_step("i1", "develop", Origin::Step)));
    tally.add("s1", "2026-11-23", &call(0.5, Attribution::of_step("i1", "develop", Origin::Summary)));
    tally.add("chat", "2026-11-23", &call(0.25, Attribution { origin: Origin::Title, ..Attribution::default() }));
    let listed = listing(7, &window, &tally.report(), &[], domain::Language::default());
    let origins: Vec<(&str, f64)> = listed["byOrigin"]
        .as_array()
        .expect("byOrigin")
        .iter()
        .map(|row| (row["id"].as_str().unwrap_or_default(), row["costUsd"].as_f64().unwrap_or_default()))
        .collect();
    assert_eq!(origins, [("step", 1.0), ("summary", 0.5), ("title", 0.25)]);
    assert_eq!(listed["unattributed"]["costUsd"], 0.25);
    assert_eq!(listed["unattributed"]["calls"], 1);
}

fn unpriced(model: &str, provider: &str) -> EngineEvent {
    EngineEvent::Usage(ModelUsage {
        model: model.into(),
        input: 1_000_000,
        output: 1_000_000,
        provider: provider.into(),
        ..ModelUsage::default()
    })
}

fn cost_of(event: &EngineEvent) -> Option<f64> {
    match event {
        EngineEvent::Usage(usage) => usage.estimated_cost_usd,
        EngineEvent::AgentEvent { event, .. } => cost_of(event),
        _ => None,
    }
}

#[test]
fn a_call_recorded_without_a_price_is_priced_from_its_tokens() {
    let mut served = unpriced("openai-subscription:gpt-5.6-terra", "openai-subscription");
    priced(&mut served);
    assert!(cost_of(&served).is_some_and(|cost| cost > 0.0));
    let mut inferred =
        EngineEvent::AgentEvent { index: 1, event: Box::new(unpriced("openai-subscription:gpt-5.6-terra", "")) };
    priced(&mut inferred);
    assert_eq!(cost_of(&inferred), cost_of(&served));
}

#[test]
fn a_recorded_price_is_kept_and_an_unknown_model_stays_unpriced() {
    let mut kept = EngineEvent::Usage(ModelUsage { estimated_cost_usd: Some(0.5), ..ModelUsage::default() });
    priced(&mut kept);
    assert_eq!(cost_of(&kept), Some(0.5));
    let mut unknown = unpriced("modelo-sem-preco", "nenhum");
    priced(&mut unknown);
    assert_eq!(cost_of(&unknown), None);
}
