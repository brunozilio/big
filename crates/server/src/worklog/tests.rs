use database::args;
use domain::billing::Cost;
use domain::origin::Origin;
use domain::principal::{Principal, UserId};
use domain::workflow::{StepState, WorkflowEvent};
use serde_json::json;

use super::*;

const DAY: i64 = domain::workflow::ledger::DAY_MS;

fn made(store: &Store, id: &str, owner: &str, title: &str) {
    let created = WorkflowEvent::ItemCreated {
        id: ItemId::from(id),
        workflow: "feature".into(),
        parent: None,
        input: json!({ "title": title, "owner": owner }),
    };
    store.append(&ItemId::from(id), &[created]).expect("created");
}

fn moved(store: &Store, id: &str, at: i64, events: &[WorkflowEvent]) {
    let item = ItemId::from(id);
    let before = store.item(&item).expect("read").map_or(0, |item| item.version);
    store.append(&item, events).expect("appended");
    store
        .database()
        .write(|conn| {
            conn.execute("UPDATE work_events SET at = ?1 WHERE item = ?2 AND seq > ?3", args![at, id, before])
        })
        .expect("dated");
}

fn finished(step: &str) -> WorkflowEvent {
    WorkflowEvent::StepFinished { step: step.into(), branch: None, state: StepState::Passed, output: json!(null) }
}

fn started(step: &str) -> WorkflowEvent {
    WorkflowEvent::StepStarted { step: step.into(), branch: None }
}

fn spent(store: &Store, at: i64, item: &str, step: &str, owner: &str, micros: u64) {
    let spent = store::Spent {
        at,
        item: item.into(),
        step: step.into(),
        owner: owner.into(),
        origin: Origin::Step,
        cost: Cost { known_micros: micros, ..Cost::default() },
        ..store::Spent::default()
    };
    store.record_spend(&spent).expect("spent");
}

fn person(store: &Store, id: &str, email: &str) {
    let principal = Principal { user: UserId(id.into()), email: email.into(), name: String::new() };
    store.ensure_user(&principal, 0).expect("user");
}

fn ledger_of(store: Arc<Store>, shared: bool) -> StoredWorklog {
    let redactor = connectors::redaction::redactor(std::iter::empty::<&dyn connectors::Provider>());
    StoredWorklog::new(store, shared, 31, redactor)
}

fn yesterday() -> Ask {
    Ask { person: None, since_ms: DAY, until_ms: 2 * DAY, limit: 20 }
}

fn two_people() -> Arc<Store> {
    let store = Arc::new(Store::in_memory(0).expect("store"));
    person(&store, "u-ana", "ana@example.com");
    person(&store, "u-bia", "bia@example.com");
    made(&store, "i-ana", "u-ana", "Corrigir o login");
    moved(&store, "i-ana", 10, &[started("plan")]);
    moved(&store, "i-ana", DAY + 10, &[finished("plan"), started("develop")]);
    made(&store, "i-bia", "u-bia", "Nova tela de busca");
    moved(&store, "i-bia", DAY + 20, &[started("plan")]);
    spent(&store, DAY + 5, "i-ana", "develop", "u-ana", 300_000);
    spent(&store, DAY + 6, "", "", "u-ana", 20_000);
    spent(&store, DAY + 7, "i-bia", "plan", "u-bia", 900_000);
    store
}

#[test]
fn a_person_reads_what_they_did_yesterday_and_what_is_left() {
    let ledger = ledger_of(two_people(), true);
    let views = ledger.items("u-ana", &yesterday()).expect("items");
    let ids: Vec<&str> = views.iter().map(|view| view.id.as_str()).collect();
    assert_eq!(ids, ["i-ana"], "only her own items");
    let view = &views[0];
    assert_eq!(view.title, "Corrigir o login");
    let done = serde_json::to_value(&view.checkpoint).expect("json");
    assert_eq!(
        done["work_done"],
        json!([{ "kind": "step", "step": "plan", "branch": null, "state": "passed", "at_ms": DAY + 10 }])
    );
    assert_eq!(done["next_steps"], json!([{ "kind": "running", "step": "develop", "branch": null }]));
    assert!((view.cost.usd - 0.3).abs() < 1e-9);
    let cost = ledger.cost("u-ana", &yesterday()).expect("cost");
    assert_eq!(cost.person.as_deref(), Some("u-ana"));
    assert!((cost.total.usd - 0.32).abs() < 1e-9, "{cost:?}");
    assert!((cost.conversations.usd - 0.02).abs() < 1e-9);
    assert_eq!(cost.items.len(), 1);
    assert_eq!(cost.items[0].title, "Corrigir o login");
}

#[test]
fn reading_someone_else_without_the_grant_is_denied() {
    let store = two_people();
    let ledger = ledger_of(Arc::clone(&store), true);
    let theirs = Ask { person: Some("u-bia".into()), ..yesterday() };
    let by_email = Ask { person: Some("bia@example.com".into()), ..yesterday() };
    let nobody = Ask { person: Some("ninguem@example.com".into()), ..yesterday() };
    let denied = ledger.items("u-ana", &theirs).expect_err("not hers");
    assert_eq!(
        ledger.items("u-ana", &nobody).expect_err("no such account").replace("ninguem@example.com", "?"),
        denied.replace("u-bia", "?"),
        "whether the account exists stays hidden"
    );
    assert!(ledger.items("u-ana", &Ask { person: Some("ana@example.com".into()), ..yesterday() }).is_ok());
    for ask in [&theirs, &by_email] {
        let refused = ledger.items("u-ana", ask).expect_err("not hers");
        assert!(refused.contains("admin"), "{refused}");
        assert!(ledger.cost("u-ana", ask).is_err());
    }
    let hidden = ledger.item("u-ana", "i-bia", &yesterday()).expect_err("hidden");
    let missing = ledger.item("u-ana", "i-nobody", &yesterday()).expect_err("missing");
    assert_eq!(hidden.replace("i-bia", "?"), missing.replace("i-nobody", "?"), "a hidden item reads as a missing one");
    person(&store, "u-chefe", "chefe@example.com");
    let admin = crate::roles::roles().held(true, &[]);
    store.set_roles(&UserId("u-chefe".into()), crate::roles::ASSIGNED, &admin).expect("roles");
    let views = ledger.items("u-chefe", &theirs).expect("an administrator reads anyone");
    assert_eq!(views.iter().map(|view| view.id.as_str()).collect::<Vec<_>>(), ["i-bia"]);
    assert_eq!(ledger.item("u-chefe", "i-bia", &yesterday()).expect("item").owner, "u-bia");
    assert!(ledger.items("u-ana", &Ask { person: Some("u-ana".into()), ..yesterday() }).is_ok());
}

#[test]
fn in_local_mode_one_person_sees_everything() {
    let ledger = ledger_of(two_people(), false);
    let views = ledger.items("local", &yesterday()).expect("items");
    let mut ids: Vec<&str> = views.iter().map(|view| view.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["i-ana", "i-bia"]);
    assert!(ledger.item("local", "i-bia", &yesterday()).is_ok());
    let cost = ledger.cost("local", &yesterday()).expect("cost");
    assert!((cost.total.usd - 1.22).abs() < 1e-9, "{cost:?}");
    assert_eq!(cost.person, None);
}

#[test]
fn an_ended_item_outside_the_window_is_left_out() {
    let store = two_people();
    moved(&store, "i-bia", 3 * DAY, &[WorkflowEvent::Cancelled { reason: "duplicado".into() }]);
    let ledger = ledger_of(store, false);
    let views = ledger.items("local", &Ask { since_ms: 4 * DAY, until_ms: 5 * DAY, ..yesterday() }).expect("items");
    assert_eq!(views.iter().map(|view| view.id.as_str()).collect::<Vec<_>>(), ["i-ana"], "still open, so still listed");
    assert!(views[0].checkpoint.work_done.is_empty());
}

#[test]
fn the_period_has_a_ceiling_and_free_text_is_redacted() {
    let store = two_people();
    made(&store, "i-old", "u-ana", "Bearer abcdef123456 no titulo");
    moved(&store, "i-old", 10, &[WorkflowEvent::Cancelled { reason: "password: hunter22".into() }]);
    let ledger = ledger_of(Arc::clone(&store), true);
    let everything = Ask { since_ms: 0, until_ms: 40 * DAY, ..yesterday() };
    let views = ledger.items("u-ana", &everything).expect("items");
    assert!(views.iter().all(|view| view.id.as_str() != "i-old"), "an ended item older than the ceiling is not read");
    let old = ledger.item("u-ana", "i-old", &Ask { since_ms: 0, until_ms: 20, ..yesterday() }).expect("item");
    let shown = serde_json::to_string(&old).expect("json");
    assert!(!shown.contains("abcdef123456") && !shown.contains("hunter22"), "{shown}");
    let limited = ledger.items("u-ana", &Ask { limit: 1, ..yesterday() }).expect("limited");
    assert_eq!(limited.len(), 1);
}

fn teams() -> crate::roles::Roles {
    let pairs = [
        ("admin", "use author admin"),
        ("member", "use"),
        ("lead", "use team"),
        ("pagamentos", "use"),
        ("busca", "use"),
    ];
    let access = crate::roles::Access {
        default_role: "member",
        admin_role: "admin",
        sso_claim: "groups",
        sso_groups: "time-pagamentos=pagamentos, time-busca=busca, lideres=lead",
    };
    crate::roles::Roles::of(&pairs, &access)
}

fn in_teams(store: &Store, id: &str, roles: &[&str]) {
    if store.user(&UserId(id.into())).expect("read").is_none() {
        person(store, id, &format!("{id}@example.com"));
    }
    let roles: Vec<String> = roles.iter().map(|role| (*role).to_owned()).collect();
    store.set_roles(&UserId(id.into()), crate::roles::SSO, &roles).expect("roles");
}

fn team_world() -> StoredWorklog {
    let store = two_people();
    in_teams(&store, "u-ana", &["pagamentos"]);
    in_teams(&store, "u-bia", &["busca"]);
    in_teams(&store, "u-leo", &["lead", "pagamentos"]);
    in_teams(&store, "u-carla", &["lead", "busca"]);
    in_teams(&store, "u-mia", &["pagamentos"]);
    ledger_of(store, true).with_roles(teams())
}

fn of(person: &str) -> Ask {
    Ask { person: Some(person.into()), ..yesterday() }
}

#[test]
fn a_team_lead_reads_the_items_and_cost_of_their_team() {
    let ledger = team_world();
    let views = ledger.items("u-leo", &of("u-ana")).expect("ana is in the lead's team");
    assert_eq!(views.iter().map(|view| view.id.as_str()).collect::<Vec<_>>(), ["i-ana"]);
    assert!(ledger.items("u-leo", &of("ana@example.com")).is_ok(), "by e-mail too");
    assert_eq!(ledger.item("u-leo", "i-ana", &yesterday()).expect("item").owner, "u-ana");
    let cost = ledger.cost("u-leo", &of("u-ana")).expect("cost");
    assert!((cost.total.usd - 0.32).abs() < 1e-9, "{cost:?}");
}

#[test]
fn a_team_lead_does_not_read_someone_outside_the_team() {
    let ledger = team_world();
    for outside in ["u-bia", "u-carla", "ninguem@example.com"] {
        let refused = ledger.items("u-leo", &of(outside)).expect_err("outside the team");
        assert!(refused.contains("team"), "{refused}");
        assert!(ledger.cost("u-leo", &of(outside)).is_err());
    }
    assert!(ledger.item("u-leo", "i-bia", &yesterday()).is_err(), "sharing the lead role is not a team");
}

#[test]
fn a_member_without_the_team_grant_reads_no_one_else() {
    let ledger = team_world();
    for teammate in ["u-ana", "u-leo", "u-bia"] {
        assert!(ledger.items("u-mia", &of(teammate)).is_err(), "{teammate}");
        assert!(ledger.cost("u-mia", &of(teammate)).is_err(), "{teammate}");
    }
    assert!(ledger.item("u-mia", "i-ana", &yesterday()).is_err());
    assert!(ledger.items("u-mia", &yesterday()).expect("her own").is_empty());
}

#[test]
fn the_default_role_and_the_leading_roles_do_not_make_a_team() {
    let roles = teams();
    assert!(access::forms_a_team(&roles, "pagamentos"));
    assert!(!access::forms_a_team(&roles, "member"));
    assert!(!access::forms_a_team(&roles, "lead"));
    assert!(!access::forms_a_team(&roles, "admin"));
    let leader = ["lead".to_owned(), "pagamentos".to_owned()];
    assert!(access::same_team(&roles, &leader, &["pagamentos".to_owned()]));
    assert!(!access::same_team(&roles, &leader, &["lead".to_owned(), "busca".to_owned()]));
}
