use serde_json::json;

use super::*;

const GIVEN: &str = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";

fn called(traceparent: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "_meta": { TRACEPARENT: traceparent } } })
}

#[test]
fn a_well_formed_traceparent_is_read_from_the_call_s_meta() {
    assert_eq!(traceparent(&called(json!(GIVEN))), Some(GIVEN));
    assert_eq!(traceparent(&json!({ "id": 1, "method": "tools/list" })), None, "no meta, no trace");
}

#[test]
fn a_malformed_traceparent_is_ignored() {
    for broken in [
        json!("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331"),
        json!("00-0af7651916cd43dd8448eb211c80319-b7ad6b7169203331-01"),
        json!("00-0af7651916cd43dd8448eb211c80319c-b7ad6b716920333z-01"),
        json!("0-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"),
        json!("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01-extra"),
        json!(""),
        json!(7),
    ] {
        assert_eq!(traceparent(&called(broken.clone())), None, "{broken}");
    }
}

#[test]
fn a_json_rpc_batch_gives_the_first_call_that_carries_one() {
    let batch = json!([{ "jsonrpc": "2.0", "method": "notifications/initialized" }, called(json!(GIVEN))]);
    assert_eq!(traceparent(&batch), Some(GIVEN));
    let broken_first = json!([called(json!("bad")), called(json!(GIVEN))]);
    assert_eq!(traceparent(&broken_first), None, "the first traceparent found decides, and a bad one is ignored");
    assert_eq!(traceparent(&json!([])), None);
}
