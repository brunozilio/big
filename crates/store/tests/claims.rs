#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

#[macro_use]
mod common;

use common::Fresh;
use store::Claim;

fn by(person: &str, at: i64) -> Claim {
    Claim { target: "i-1:approve".into(), item: "i-1".into(), person: person.into(), name: person.to_uppercase(), at }
}

fn the_first_claim_holds_and_the_others_see_who_took_it(fresh: Fresh) {
    let place = fresh();
    let store = place.open(0);
    assert_eq!(store.claim_inbox(&by("ana", 10)).expect("ana"), by("ana", 10));
    assert_eq!(store.claim_inbox(&by("bia", 20)).expect("bia"), by("ana", 10));
    assert_eq!(store.inbox_claims().expect("listed"), vec![by("ana", 10)]);
    assert!(!store.release_inbox("i-1:approve", Some("bia")).expect("not hers"));
    assert!(store.release_inbox("i-1:approve", Some("ana")).expect("hers"));
    assert_eq!(store.claim_inbox(&by("bia", 30)).expect("bia again"), by("bia", 30));
    assert!(store.release_inbox("i-1:approve", None).expect("anyone's"));
    assert!(store.inbox_claims().expect("empty").is_empty());
    store.claim_inbox(&by("ana", 40)).expect("again");
    let other = Claim { target: "ask-9".into(), item: "i-2".into(), ..by("bia", 50) };
    store.claim_inbox(&other).expect("other item");
    assert_eq!(store.forget_item_claims("i-1").expect("forgotten"), 1);
    assert_eq!(store.inbox_claims().expect("left"), vec![other]);
}

fn many_instances_open_the_same_new_database_at_once(fresh: Fresh) {
    let place = fresh();
    if matches!(place, common::Place::File { .. }) {
        return;
    }
    std::thread::scope(|scope| {
        let opened: Vec<_> = (0..6).map(|_| scope.spawn(|| place.open(0).inbox_claims().map(|found| found.len()))).collect();
        for each in opened {
            assert_eq!(each.join().expect("joined").expect("migrated"), 0);
        }
    });
}

scenarios!(the_first_claim_holds_and_the_others_see_who_took_it, many_instances_open_the_same_new_database_at_once);
