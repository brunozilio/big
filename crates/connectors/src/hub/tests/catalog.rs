use super::*;

#[tokio::test]
async fn a_polled_event_takes_the_declared_name_and_keeps_the_old_one_as_alias() {
    let hub = hub(FakeHttp::new(), Some(Grant::bearer("tok"))).await;
    let batch = hub.poll(&connection(&["x", "y"]), &json!({}), Some("c"), &[]).await;
    let Ok(Some(batch)) = batch else { panic!("{batch:?}") };
    let names: Vec<Vec<&str>> = batch.events.iter().map(ExternalEvent::names).collect();
    assert_eq!(names, [vec!["echo.x.changed", "x.changed"], vec!["y.changed"]]);
    assert!(batch.events[0].answers("x.changed") && batch.events[0].answers("echo.x.changed"));
}

#[tokio::test]
async fn the_catalog_lists_the_declared_events_and_finds_one_by_any_name() {
    let hub = hub(FakeHttp::new(), None).await;
    let listed = hub.catalog(Language::default());
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].connector.as_str(), "echo");
    assert_eq!(listed[0].events[0].name, "echo.x.changed");
    assert_eq!(hub.event("x.changed").map(|spec| spec.name.as_str()), Some("echo.x.changed"));
    assert_eq!(hub.event("echo.x.changed").map(|spec| spec.name.as_str()), Some("echo.x.changed"));
    assert!(hub.event("y.changed").is_none());
    let mut synthetic = batch_event("x.changed");
    hub.canonical(&ProviderKey::from("echo"), &mut synthetic);
    assert_eq!(synthetic.kind, "echo.x.changed");
}

fn batch_event(kind: &str) -> ExternalEvent {
    ExternalEvent {
        id: "e".into(),
        connection: ConnectionId::from("c1"),
        capability: Capability::from("x"),
        kind: kind.into(),
        subject: "s".into(),
        data: json!({}),
        at: jiff::Timestamp::UNIX_EPOCH,
        scope: String::new(),
        aliases: Vec::new(),
    }
}
