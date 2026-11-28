use serde_json::json;

use super::*;

fn command(args: serde_json::Value) -> Command {
    serde_json::from_value(json!({ "seq": 1, "kind": "agent", "branch": "", "args": args })).expect("command")
}

fn usage(usd: f64) -> EngineEvent {
    EngineEvent::Usage(domain::billing::ModelUsage { estimated_cost_usd: Some(usd), ..Default::default() })
}

fn charge() -> Charge {
    Charge::new(&ItemId::from("i1"), "develop", "dev", Some("app-web".into()), "u-ana")
}

const MONTH: Month = Month { since_ms: 0, resets_at: 1_800_000_000 };

#[test]
fn a_run_is_charged_to_its_item_and_step_and_to_the_origin_the_workflow_names() {
    let item = ItemId::from("i1");
    let plain = attribution(&item, "develop", &command(json!({ "name": "dev" })));
    assert_eq!(plain, Attribution::of_step("i1", "develop", Origin::Step));
    let eval = attribution(&item, "evals", &command(json!({ "name": "judge", "origin": "eval" })));
    assert_eq!(eval.origin, Origin::Eval);
    let unknown = attribution(&item, "x", &command(json!({ "origin": "whatever" })));
    assert_eq!(unknown.origin, Origin::Step);
}

#[test]
fn without_any_ceiling_a_run_gets_no_budget_at_all() {
    let store = Store::in_memory(0).expect("store");
    let none = budgets(&store, &Ceilings::default(), &charge(), &command(json!({})), Some(MONTH));
    assert!(none.is_empty(), "{none:?}");
}

#[test]
fn every_ceiling_set_becomes_a_budget_with_what_its_scope_spent_this_month() {
    let store = Store::in_memory(0).expect("store");
    let earlier = charge();
    earlier.record(&store, &usage(1.25), 10);
    earlier.record(&store, &EngineEvent::TextDelta("hi".into()), 11);
    let ceilings = Ceilings { run: Some(5.0), agent: Some(20.0), application: Some(50.0), person: Some(30.0) };
    let found = budgets(&store, &ceilings, &earlier, &command(json!({ "budgetUsd": 2 })), Some(MONTH));
    let scopes: Vec<(Scope, f64, u64)> = found.iter().map(|b| (b.scope, b.limit_usd, b.spent.known_micros)).collect();
    assert_eq!(
        scopes,
        [
            (Scope::Run, 2.0, 1_250_000),
            (Scope::Agent, 20.0, 1_250_000),
            (Scope::Application, 50.0, 1_250_000),
            (Scope::Person, 30.0, 1_250_000)
        ]
    );
    assert_eq!(found[1].resets_at, Some(MONTH.resets_at));
    assert_eq!(found[0].resets_at, None);
    let fresh = budgets(&store, &ceilings, &charge(), &command(json!({})), Some(MONTH));
    assert_eq!((fresh[0].limit_usd, fresh[0].spent.known_micros), (5.0, 0), "a new run starts at zero");
}

#[test]
fn an_item_without_an_application_has_no_application_ceiling() {
    let store = Store::in_memory(0).expect("store");
    let alone = Charge::new(&ItemId::from("i2"), "develop", "dev", None, "u-ana");
    let ceilings = Ceilings { application: Some(10.0), ..Ceilings::default() };
    assert!(budgets(&store, &ceilings, &alone, &command(json!({})), Some(MONTH)).is_empty());
}

#[test]
fn the_agent_s_own_ceiling_bounds_its_runs_when_the_step_names_none() {
    let store = Store::in_memory(0).expect("store");
    let declared = charge().with_agent_ceiling(Some(0.75));
    let found = budgets(&store, &Ceilings::default(), &declared, &command(json!({})), None);
    assert_eq!(found.iter().map(|b| (b.scope, b.limit_usd)).collect::<Vec<_>>(), [(Scope::Run, 0.75)]);
}

#[test]
fn a_month_runs_from_its_first_local_midnight_to_the_next_one() {
    let now: Zoned = "2026-11-28T15:30:00-03:00[America/Sao_Paulo]".parse().expect("moment");
    let month = Month::of(&now).expect("month");
    let start: Zoned = "2026-11-01T00:00:00-03:00[America/Sao_Paulo]".parse().expect("start");
    let next: Zoned = "2026-12-01T00:00:00-03:00[America/Sao_Paulo]".parse().expect("next");
    assert_eq!(month.since_ms, start.timestamp().as_millisecond());
    assert_eq!(month.resets_at, next.timestamp().as_second());
}
