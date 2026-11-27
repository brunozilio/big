use serde_json::json;

use super::*;
use crate::approval::PermissionMode;

fn roles(pairs: &[(&str, &str)]) -> ModelRoles {
    let mut roles = ModelRoles::default();
    for (role, model) in pairs {
        roles.set(role, Some((*model).to_owned()));
    }
    roles
}

fn shipped() -> Tiers {
    Tiers::read(&json!([
        { "name": "main", "about": "the session's model", "session": true },
        { "name": "plan", "about": "plan mode" },
        { "name": "explore", "about": "the Explore agent", "fallback": "small" },
        { "name": "small", "about": "chores" },
    ]))
}

#[test]
fn tiers_are_read_from_content_and_the_session_tier_is_no_role() {
    let tiers = Tiers::read(&json!([
        { "name": " Review ", "about": "reviews", "fallback": " Plan " },
        { "name": "review", "about": "a repeat is left out" },
        { "name": "" },
        "not a tier",
        { "name": "main", "session": true },
    ]));
    let names: Vec<&str> = tiers.all().iter().map(|spec| spec.name.as_str()).collect();
    assert_eq!(names, ["review", "main"]);
    assert_eq!(tiers.find("REVIEW").and_then(|spec| spec.fallback.as_deref()), Some("plan"));
    assert_eq!(tiers.role(" Review "), Some(ModelRole::named("review")));
    assert_eq!(tiers.role("main"), None);
    assert_eq!(tiers.roles().count(), 1);
}

#[test]
fn planning_turns_use_the_plan_model_and_the_rest_use_the_session_model() {
    let set = roles(&[(PLAN, "claude:opus")]);
    assert_eq!(set.for_turn("grok:grok-4", PermissionMode::Plan), "claude:opus");
    assert_eq!(set.for_turn("grok:grok-4", PermissionMode::Auto), "grok:grok-4");
    assert_eq!(ModelRoles::default().for_turn("grok:grok-4", PermissionMode::Plan), "grok:grok-4");
}

#[test]
fn the_plan_and_explore_agents_take_their_role_and_explore_falls_back_to_the_small_model() {
    let set = roles(&[(PLAN, "p")]);
    assert_eq!(set.for_agent("Plan", Some("s")), Some("p"));
    assert_eq!(set.for_agent("Explore", Some("s")), Some("s"));
    assert_eq!(roles(&[(EXPLORE, "e")]).for_agent("Explore", Some("s")), Some("e"));
    assert_eq!(set.for_agent("general-purpose", Some("s")), None);
}

#[test]
fn clearing_a_role_forgets_it_and_blank_names_are_never_kept() {
    let mut set = roles(&[(PLAN, "p")]);
    set.set(PLAN, None);
    set.set(SMALL, Some("  ".into()));
    assert_eq!(set, ModelRoles::default());
}

#[test]
fn roles_round_trip_as_an_object_keyed_by_role_name() {
    let set = roles(&[("Explore", "e"), (SMALL, "s"), ("review", "r")]);
    let json = serde_json::to_value(&set).expect("serialize");
    assert_eq!(json, json!({"explore": "e", "review": "r", "small": "s"}));
    assert_eq!(serde_json::from_value::<ModelRoles>(json).expect("deserialize"), set);
}

#[test]
fn a_tier_named_by_content_resolves_to_the_role_s_model_or_else_the_session_s() {
    let tiers = shipped();
    let set = roles(&[(PLAN, "p")]);
    assert_eq!(set.for_tier(&tiers, " Main ", "main", Some("tiny")), "main");
    assert_eq!(set.for_tier(&tiers, "plan", "main", Some("tiny")), "p");
    assert_eq!(set.for_tier(&tiers, "explore", "main", Some("tiny")), "tiny");
    assert_eq!(set.for_tier(&tiers, "small", "main", Some("tiny")), "tiny");
    assert_eq!(ModelRoles::default().for_tier(&tiers, "small", "main", None), "main");
    assert_eq!(ModelRoles::default().for_tier(&tiers, "plan", "main", None), "main");
    assert_eq!(roles(&[(SMALL, "s")]).for_tier(&tiers, "small", "main", None), "s");
}

#[test]
fn a_tier_the_user_adds_resolves_through_its_fallback_and_a_loop_ends_at_the_session() {
    let tiers = Tiers::read(&json!([
        { "name": "review", "fallback": "plan" },
        { "name": "plan" },
        { "name": "a", "fallback": "b" },
        { "name": "b", "fallback": "a" },
    ]));
    assert_eq!(roles(&[("review", "r")]).for_tier(&tiers, "review", "main", None), "r");
    assert_eq!(roles(&[(PLAN, "p")]).for_tier(&tiers, "review", "main", None), "p");
    assert_eq!(ModelRoles::default().for_tier(&tiers, "review", "main", None), "main");
    assert_eq!(ModelRoles::default().for_tier(&tiers, "a", "main", None), "main");
    assert_eq!(ModelRoles::default().for_tier(&tiers, "unknown", "main", None), "main");
}

#[test]
fn a_role_is_parsed_against_the_declared_tiers() {
    Tiers::declare(Arc::new(shipped()));
    assert_eq!(ModelRole::parse(" Explore "), Some(ModelRole::named("explore")));
    assert_eq!(ModelRole::parse("main"), None);
    assert_eq!(ModelRole::parse("implement"), None);
}
