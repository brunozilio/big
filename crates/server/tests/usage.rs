#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

mod support;

use support::World;

#[tokio::test(flavor = "multi_thread")]
async fn usage_lists_every_day_of_the_window_and_falls_back_to_a_month() {
    let world = World::new().await;
    let week = world.get("/api/usage?days=7").await;
    assert_eq!(week["days"], 7);
    assert_eq!(week["daily"].as_array().map(Vec::len), Some(7));
    assert_eq!(week["daily"][6]["date"], week["to"]);
    assert_eq!(week["daily"][0]["date"], week["from"]);
    for key in ["costUsd", "input", "output", "cacheRead", "cacheWrite", "calls", "sessions"] {
        assert!(week["totals"][key].is_number(), "{key}: {week}");
    }
    for key in ["byProvider", "byModel", "byAccount"] {
        assert!(week[key].is_array(), "{key}: {week}");
    }
    let odd = world.get("/api/usage?days=12").await;
    assert_eq!(odd["days"], 30);
    assert_eq!(odd["daily"].as_array().map(Vec::len), Some(30));
}
