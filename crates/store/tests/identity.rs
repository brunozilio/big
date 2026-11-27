#![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

use domain::principal::{Principal, UserId};
use store::{CodeCheck, Passkey, Store};

fn store() -> Store {
    Store::in_memory(0).expect("store")
}

fn ana() -> Principal {
    Principal { user: UserId("u-ana".into()), email: "ana@example.com".into(), name: "Ana".into() }
}

#[test]
fn a_user_is_kept_once_per_email_and_keeps_the_first_id() {
    let store = store();
    let first = store.ensure_user(&ana(), 10).expect("first");
    let again = Principal { user: UserId("u-other".into()), name: String::new(), ..ana() };
    let second = store.ensure_user(&again, 20).expect("second");
    assert_eq!(first, second);
    assert_eq!(store.user_by_email("ana@example.com").expect("found"), Some(ana()));
}

#[test]
fn a_session_lasts_until_it_expires_or_is_closed() {
    let store = store();
    store.ensure_user(&ana(), 0).expect("user");
    store.open_session("t1", &ana().user, 0, 100).expect("open");
    assert_eq!(store.session_user("t1", 50).expect("read"), Some(ana()));
    assert_eq!(store.session_user("t1", 100).expect("read"), None);
    store.close_session("t1").expect("close");
    assert_eq!(store.session_user("t1", 50).expect("read"), None);
}

#[test]
fn a_code_works_once_and_gives_out_after_too_many_wrong_tries() {
    let store = store();
    store.put_code("ana@example.com", "h1", 100).expect("put");
    assert_eq!(store.check_code("ana@example.com", "nope", 10, 2).expect("check"), CodeCheck::Wrong);
    assert_eq!(store.check_code("ana@example.com", "h1", 10, 2).expect("check"), CodeCheck::Matched);
    assert_eq!(store.check_code("ana@example.com", "h1", 10, 2).expect("check"), CodeCheck::Gone);
    store.put_code("ana@example.com", "h2", 100).expect("put");
    for _ in 0..2 {
        store.check_code("ana@example.com", "nope", 10, 2).expect("check");
    }
    assert_eq!(store.check_code("ana@example.com", "h2", 10, 2).expect("check"), CodeCheck::Gone);
}

#[test]
fn a_challenge_is_taken_once_and_only_by_its_kind() {
    let store = store();
    store.put_challenge("c1", "sso", "{}", 100).expect("put");
    assert_eq!(store.take_challenge("c1", "passkey", 10).expect("take"), None);
    store.put_challenge("c2", "sso", "{}", 100).expect("put");
    assert_eq!(store.take_challenge("c2", "sso", 10).expect("take"), Some("{}".into()));
    assert_eq!(store.take_challenge("c2", "sso", 10).expect("take"), None);
}

#[test]
fn passkeys_belong_to_one_user() {
    let store = store();
    let key = Passkey {
        id: "k1".into(),
        user: ana().user,
        public_key: vec![1, 2, 3],
        algorithm: -7,
        counter: 0,
        name: "Mac".into(),
        created: 5,
        used: None,
    };
    store.add_passkey(&key).expect("add");
    store.used_passkey("k1", 7, 9).expect("used");
    let kept = store.passkey("k1").expect("read").expect("there");
    assert_eq!((kept.counter, kept.used), (7, Some(9)));
    assert_eq!(store.passkeys(&ana().user).expect("list").len(), 1);
    assert!(!store.remove_passkey(&UserId("u-other".into()), "k1").expect("remove"));
    assert!(store.remove_passkey(&ana().user, "k1").expect("remove"));
}

#[test]
fn what_nobody_claimed_belongs_to_the_local_user() {
    let store = store();
    store.set_owner("connection", "c-ana", "u-ana").expect("own");
    assert_eq!(store.owner_of("connection", "c-old").expect("owner"), "local");
    let local = store.owned_ids("connection", "local").expect("ids");
    assert!(local.contains("c-old") && !local.contains("c-ana"));
    let ana = store.owned_ids("connection", "u-ana").expect("ids");
    assert!(ana.contains("c-ana") && !ana.contains("c-old"));
    assert_eq!(store.claim("session", "s1", "u-ana").expect("claim"), "u-ana");
    assert_eq!(store.claim("session", "s1", "u-bia").expect("claim"), "u-ana");
}

#[test]
fn each_person_keeps_their_own_language_over_the_shared_one() {
    let store = store();
    store.set_branding_for("local", "language", &serde_json::json!("en")).expect("set");
    store.set_branding_for("u-ana", "language", &serde_json::json!("es")).expect("set");
    assert_eq!(store.language().expect("language"), domain::Language::English);
    assert_eq!(store.language_for("u-ana").expect("language"), domain::Language::Spanish);
    assert_eq!(store.language_for("u-bia").expect("language"), domain::Language::English);
    assert!(!store.branding().expect("branding").keys().any(|key| key.starts_with("user.")));
}
