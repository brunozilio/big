#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use common::Fresh;

fn each_person_keeps_one_number_and_no_two_share_it(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    let ana = store.agent_number("ana").expect("ana");
    let bia = store.agent_number("bia").expect("bia");
    assert_ne!(ana, bia);
    assert_eq!(store.agent_number("ana").expect("ana again"), ana);
    let reopened = place.open(0);
    assert_eq!(reopened.agent_number("bia").expect("bia after reopening"), bia);
    assert!(ana >= 1 && bia >= 1);
}

scenarios!(each_person_keeps_one_number_and_no_two_share_it);
