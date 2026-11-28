use super::*;

const TOKEN: &str = "echo_0123456789";

async fn echoing() -> Connectors {
    hub(FakeHttp::new().always(Method::Get, "/echo", 200, "{}"), Some(Grant::bearer("tok"))).await
}

#[tokio::test]
async fn a_write_with_a_secret_outside_its_secret_fields_does_not_run_and_names_the_field() {
    let hub = echoing().await;
    let before = ECHO.calls.load(Ordering::SeqCst);
    let args = json!({ "title": "Deploy", "body": format!("deploy with {TOKEN}") });
    let refused = hub.call(&connection(&["x"]), &json!({}), "x.send", &args, "k1").await;
    assert_eq!(refused.as_ref().map_err(ActionError::secret_field), Err(Some("body")), "{refused:?}");
    let nested = json!({ "fields": [{ "note": "password=hunter22" }] });
    let refused = hub.call(&connection(&["x"]), &json!({}), "x.send", &nested, "k2").await;
    assert_eq!(refused.as_ref().map_err(ActionError::secret_field), Err(Some("fields")));
    assert_eq!(ECHO.calls.load(Ordering::SeqCst), before, "the provider was never called");
    let plain =
        hub.call(&connection(&["x"]), &json!({}), "x.send", &json!({ "body": "the token: string" }), "k3").await;
    assert!(plain.is_ok(), "{plain:?}");
}

#[tokio::test]
async fn a_field_declared_secret_carries_its_secret_and_the_others_still_cannot() {
    let hub = echoing().await;
    let kept =
        hub.call(&connection(&["x"]), &json!({}), "x.keep", &json!({ "name": "hook", "token": TOKEN }), "k1").await;
    let Ok(kept) = kept else { panic!("{kept:?}") };
    assert_eq!(kept["args"]["token"], TOKEN, "a field meant to carry a secret goes as it is");
    let args = json!({ "name": format!("hook {TOKEN}"), "token": TOKEN });
    let refused = hub.call(&connection(&["x"]), &json!({}), "x.keep", &args, "k2").await;
    assert_eq!(refused.as_ref().map_err(ActionError::secret_field), Err(Some("name")));
}

#[tokio::test]
async fn a_read_sends_nothing_out_so_its_arguments_stay() {
    let hub = echoing().await;
    let read = hub.call(&connection(&["x"]), &json!({}), "x.read", &json!({ "a": TOKEN }), "k1").await;
    let Ok(read) = read else { panic!("{read:?}") };
    assert_eq!(read["args"]["a"], TOKEN);
    let said = hub.redactor().redact(&format!("key {TOKEN}"), &domain::redact::Known::default());
    assert_eq!(said.text, "key [redacted]", "the connector's own token shape joins the hub's redactor");
}
