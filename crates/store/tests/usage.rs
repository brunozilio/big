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

scenarios!(connector_calls_add_up_per_connection_origin_and_whether_an_item_owns_them);
