use super::*;

#[tokio::test]
async fn a_secret_in_the_arguments_of_a_write_never_reaches_the_provider() {
    let hub = hub(FakeHttp::new().always(Method::Get, "/echo", 200, "{}"), Some(Grant::bearer("tok"))).await;
    let args = json!({
        "body": "deploy with ghp_0123456789abcdefghij and echo_0123456789",
        "fields": [{ "note": "password=hunter2" }],
        "title": "Deploy",
    });
    let sent = hub.call(&connection(&["x"]), &json!({}), "x.send", &args, "k1").await;
    let Ok(sent) = sent else { panic!("{sent:?}") };
    assert_eq!(
        sent["args"],
        json!({
            "body": "deploy with [redacted] and [redacted]",
            "fields": [{ "note": "password=[redacted]" }],
            "title": "Deploy",
        })
    );
    let read = hub.call(&connection(&["x"]), &json!({}), "x.read", &json!({"a": "echo_0123456789"}), "k2").await;
    let Ok(read) = read else { panic!("{read:?}") };
    assert_eq!(read["args"]["a"], "echo_0123456789", "a read sends nothing out, so its arguments stay");
}

#[test]
fn a_connector_s_secret_patterns_join_every_redaction() {
    let echo: &'static dyn Provider = &*ECHO;
    crate::redaction::declare([echo].into_iter());
    assert_eq!(domain::redact::redact("key echo_0123456789", &domain::redact::Known::default()), "key [redacted]");
}
