use serde_json::{Value, json};

use super::*;

fn all(body: Body, framing: Framing, cancel: &CancellationToken) -> Result<Vec<Value>, ModelError> {
    platform::runtime::block_on(async {
        let mut framed = Framed::new(body, framing);
        let mut events = Vec::new();
        while let Some(event) = framed.next(cancel).await? {
            events.push(event);
        }
        Ok(events)
    })
    .unwrap_or_else(|error| Err(ModelError::Network(error.to_string())))
}

fn read(text: &[u8], framing: Framing) -> Result<Vec<Value>, ModelError> {
    all(Body::from(text.to_vec()), framing, &CancellationToken::new())
}

#[test]
fn sse_events_keep_their_names_and_joined_data() {
    let text = b"event: start\ndata: {\"a\":1}\n\n: comment\ndata: one\ndata: two\n\ndata: last";
    let events = read(text, Framing::Sse);
    assert_eq!(
        events,
        Ok(vec![
            json!({ "event": "start", "data": "{\"a\":1}" }),
            json!({ "event": "", "data": "one\ntwo" }),
            json!({ "event": "", "data": "last" }),
        ])
    );
}

#[test]
fn ndjson_lines_are_values_and_a_bad_line_is_an_error() {
    assert_eq!(read(b"{\"a\":1}\n\n[2]\n", Framing::Ndjson), Ok(vec![json!({ "a": 1 }), json!([2])]));
    assert!(read(b"{\"a\":1}\nnot json\n", Framing::Ndjson).is_err());
}

#[test]
fn event_stream_frames_carry_headers_and_payload() {
    let events = read(include_bytes!("testdata/text.bin"), Framing::EventStream).unwrap_or_default();
    assert_eq!(events.len(), 9);
    assert_eq!(events[0]["headers"][":event-type"], "initial-response");
    assert!(events.iter().all(|event| event["payload"].is_string()));
}

#[test]
fn raw_text_never_splits_a_character() {
    let (sender, receiver) = tokio::sync::mpsc::channel(4);
    let bytes = "olá".as_bytes();
    let pieces = [bytes[..3].to_vec(), bytes[3..].to_vec()];
    for piece in pieces {
        let _ = sender.try_send(Ok(piece));
    }
    drop(sender);
    let events = all(Body::chunks(receiver), Framing::Raw, &CancellationToken::new());
    assert_eq!(events, Ok(vec![json!("ol"), json!("á")]));
}

#[test]
fn a_cancelled_stream_stops() {
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(all(Body::from("data: x\n\n"), Framing::Sse, &cancel), Err(ModelError::Cancelled));
}

#[test]
fn framings_are_named() {
    assert_eq!(Framing::named("aws-eventstream"), Ok(Framing::EventStream));
    assert!(Framing::named("xml").is_err());
}
