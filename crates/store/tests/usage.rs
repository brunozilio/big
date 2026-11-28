#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use common::Fresh;
use store::{ConnectorCall, ConnectorCalls};

fn call<'a>(
    at: i64,
    connection: &'a str,
    origin: Option<&'a str>,
    item: Option<&'a str>,
    ok: bool,
) -> ConnectorCall<'a> {
    ConnectorCall { at, connection, action: "tracker.issue", origin, item, step: None, ok }
}

fn row(connection: &str, origin: Option<&str>, attributed: bool, calls: u64, failed: u64) -> ConnectorCalls {
    ConnectorCalls { connection: connection.into(), origin: origin.map(str::to_owned), attributed, calls, failed }
}

fn connector_calls_add_up_per_connection_origin_and_whether_an_item_owns_them(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    for counted in [
        call(5, "c-a", Some("step"), Some("i1"), true),
        call(6, "c-a", Some("step"), Some("i2"), false),
        call(7, "c-a", None, None, true),
        call(8, "c-b", Some("trigger"), None, true),
        call(1, "c-b", Some("trigger"), None, true),
    ] {
        store.count_connector_call(&counted).expect("counted");
    }
    let found = store.connector_calls_since(5).expect("read");
    assert_eq!(
        found,
        [row("c-a", None, false, 1, 0), row("c-a", Some("step"), true, 2, 1), row("c-b", Some("trigger"), false, 1, 0)]
    );
}

fn spent(at: i64, agent: &str, application: &str, owner: &str, micros: u64, unpriced: u64) -> store::Spent {
    store::Spent {
        at,
        item: "i1".into(),
        step: "develop".into(),
        agent: agent.into(),
        application: application.into(),
        owner: owner.into(),
        origin: domain::origin::Origin::Step,
        cost: domain::billing::Cost { known_micros: micros, unpriced_calls: unpriced, ..Default::default() },
    }
}

fn spend_adds_up_per_agent_application_and_person_since_a_moment(fresh: Fresh) {
    use domain::quota::Scope;
    let place = fresh();
    let store = place.open(0);
    for kept in [
        spent(10, "dev", "app-web", "u-ana", 400_000, 0),
        spent(20, "dev", "app-api", "u-ana", 250_000, 1),
        spent(30, "planner", "app-web", "u-bia", 100_000, 0),
        spent(1, "dev", "app-web", "u-ana", 9_000_000, 0),
        spent(40, "dev", "app-web", "u-ana", 0, 0),
    ] {
        store.record_spend(&kept).expect("kept");
    }
    let of = |scope, id| store.spent_since(10, scope, id).expect("read");
    let dev = of(Scope::Agent, "dev");
    assert_eq!((dev.known_micros, dev.unpriced_calls), (650_000, 1));
    assert_eq!(of(Scope::Application, "app-web").known_micros, 500_000);
    assert_eq!(of(Scope::Person, "u-bia").known_micros, 100_000);
    assert_eq!(of(Scope::Person, "nobody").known_micros, 0);
    assert_eq!(of(Scope::Run, "dev").known_micros, 0, "a run's spend is not kept by scope");
    assert_eq!(of(Scope::Application, "").known_micros, 0, "an item without an application has none to count");
}

scenarios!(
    connector_calls_add_up_per_connection_origin_and_whether_an_item_owns_them,
    spend_adds_up_per_agent_application_and_person_since_a_moment
);
