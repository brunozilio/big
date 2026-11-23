use super::*;

#[test]
fn a_wakeup_fires_once_at_its_time() {
    let client = 7101;
    let start = now();
    add(client, "wake up", start + 90).expect("wakeup");
    add(client, "later", start + 600).expect("wakeup");

    assert!(take_due(client, start + 89).is_empty(), "nothing is due yet");
    let due: Vec<String> = take_due(client, start + 125).into_iter().map(|due| due.prompt).collect();
    assert_eq!(due, ["wake up"]);
    assert!(take_due(client, start + 125).is_empty(), "a wakeup fires once");
    let listed = list(client);
    assert_eq!(listed.len(), 1, "the later one left: {listed:?}");
    assert_eq!(listed[0].next, start + 600);
    forget_client(client);
}

#[test]
fn a_full_schedule_refuses_one_more() {
    let client = 7103;
    let start = now();
    for at in 0..MOST {
        add(client, "wake up", start + 90 + i64::try_from(at).expect("small")).expect("room");
    }
    let refused = add(client, "one more", start + 90).expect_err("full");
    assert_eq!(refused, ScheduleError::Full);
    assert_eq!(String::from(refused), format!("At most {MOST} wakeups can be scheduled."));
    forget_client(client);
}

#[test]
fn wakeups_belong_to_their_client_until_it_leaves() {
    let start = now();
    add(7104, "mine", start).expect("at");
    assert!(take_due(7105, start + 1).is_empty(), "another client's");
    add(7104, "also mine", start + 600).expect("at");
    forget_client(7104);
    assert!(list(7104).is_empty());
}

fn hours(east: i8) -> TimeZone {
    jiff::tz::Offset::from_hours(east).expect("an offset within a day").to_time_zone()
}

#[test]
fn a_wakeup_is_shown_at_the_minute_on_the_user_s_clock() {
    let early_utc_morning = 1_793_849_400;
    assert_eq!(minute_in(&hours(0), early_utc_morning).as_deref(), Some("2026-11-05 03:30"));
    assert_eq!(minute_in(&hours(-8), early_utc_morning).as_deref(), Some("2026-11-04 19:30"));
    assert_eq!(minute_in(&hours(9), early_utc_morning + 59).as_deref(), Some("2026-11-05 12:30"));
    assert_eq!(minute_in(&hours(0), 0).as_deref(), Some("1970-01-01 00:00"));
}

#[test]
fn scheduling_a_wakeup_asks_first() {
    use crate::tools::{Effect, Tool};
    let workspace = Workspace::new("/work/api");
    let acts = |tool: &dyn Tool| tool.effects(&Value::Null, &workspace).iter().any(Effect::acts);
    assert!(acts(&ScheduleWakeup), "a planted prompt would run unasked");
}
