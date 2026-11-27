use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use domain::Language;
use jiff::{SignedDuration, Timestamp};
use serde_json::{Value, json};

use super::provider;
use crate::testing::ScriptedHost;
use crate::{Config, Provider, ProviderError, Tokens};

fn chatgpt() -> &'static dyn Provider {
    provider("openai-subscription")
}

fn jwt(claims: Value) -> String {
    format!("h.{}.s", URL_SAFE_NO_PAD.encode(claims.to_string()))
}

fn shown(usage: &domain::billing::PlanUsage) -> Vec<(&str, &str, f64, Option<String>)> {
    usage.windows.iter().map(|w| (w.id.as_str(), w.label.as_str(), w.used_percent, w.resets_at.clone())).collect()
}

#[test]
fn a_conversation_s_cache_key_also_names_its_session() {
    let headers = chatgpt().turn_headers(None, &json!({"prompt_cache_key": "c-1", "input": []}));
    assert_eq!(headers, [("session_id".to_owned(), "c-1".to_owned())]);
    assert!(chatgpt().turn_headers(None, &json!({"input": []})).is_empty());
}

#[test]
fn the_account_comes_from_any_of_the_three_claims() {
    let account = |access: String| {
        let body = json!({"access_token": access, "refresh_token": "r", "expires_in": 3600}).to_string();
        chatgpt().tokens(200, &body, None, Timestamp::now()).map(|tokens| tokens.account_id)
    };
    let nested = jwt(json!({"https://api.openai.com/auth": {"chatgpt_account_id": "a1"}}));
    assert_eq!(account(nested), Ok("a1".into()));
    assert_eq!(account(jwt(json!({"chatgpt_account_id": "a2"}))), Ok("a2".into()));
    assert_eq!(account(jwt(json!({"organizations": [{"id": "a3"}]}))), Ok("a3".into()));
    assert!(account("not-a-jwt".into()).is_err());
}

#[test]
fn the_code_is_exchanged_with_a_form_without_state() {
    let account = jwt(json!({"chatgpt_account_id": "acct"}));
    let answer = json!({"access_token": account, "refresh_token": "r", "expires_in": 3600}).to_string();
    let host = ScriptedHost::new([(200, answer)], [("/auth/callback", "code=c&state=r2x16")]);
    let config = chatgpt().sign_in(&host).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(config.account(), "acct");
    let log = host.log.borrow();
    assert!(log.opened[0].0.starts_with("https://auth.openai.com/oauth/authorize?"), "{:?}", log.opened);
    let (url, body) = &log.sent[0];
    assert_eq!(url, "https://auth.openai.com/oauth/token");
    assert!(body.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann") && !body.contains("state"), "{body}");
    assert!(body.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"), "{body}");
}

#[test]
fn a_refresh_keeps_the_account_the_answer_omits() {
    let previous = Tokens { refresh_token: "r0".into(), account_id: "acct".into(), ..Tokens::bearer("old") };
    let now = Timestamp::now();
    let fresh = chatgpt().tokens(200, r#"{"access_token":"new","expires_in":60}"#, Some(&previous), now);
    let fresh = fresh.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((fresh.refresh_token.as_str(), fresh.account_id.as_str()), ("r0", "acct"));
    assert_eq!(fresh.expires_at, Some(now + SignedDuration::from_secs(60)));
}

#[test]
fn a_refresh_is_a_form_with_the_client() {
    let request = chatgpt().refresh(&Tokens { refresh_token: "r0".into(), ..Tokens::bearer("a") });
    let request = request.unwrap_or_else(|| panic!("refreshable"));
    assert_eq!(
        (request.url.as_str(), request.content_type),
        ("https://auth.openai.com/oauth/token", "application/x-www-form-urlencoded")
    );
    assert!(request.body.contains("grant_type=refresh_token") && request.body.contains("refresh_token=r0"));
}

#[test]
fn the_account_id_is_required() {
    let tokens = Tokens { refresh_token: "r".into(), ..Tokens::bearer("a") };
    let config = Config { auth: Some(tokens), ..Config::default() };
    assert!(config.validate(chatgpt()).is_err());
}

#[test]
fn codex_gets_its_instructions_effort_and_no_strict_tools() {
    let mut body = json!({"model": "openai-subscription:gpt-5", "instructions": "be brief",
        "reasoning": {"effort": "max"}, "tool_choice": "auto", "tools": [{"name": "t", "strict": true}]});
    crate::request::shape(chatgpt(), None, &mut body).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        body["instructions"]
            .as_str()
            .is_some_and(|s| s.starts_with(brand::chatgpt::INSTRUCTIONS) && s.ends_with("be brief"))
    );
    assert_eq!(body["reasoning"], json!({"effort": "xhigh", "summary": "auto"}));
    assert!(body.get("tool_choice").is_none() && body["tools"][0].get("strict").is_none());
}

#[test]
fn the_catalog_reads_slugs_windows_and_levels() {
    let page = json!({"models": [{"slug": "gpt-5", "context_window": 272000,
        "supported_reasoning_levels": [{"effort": "low"}, {"effort": "high"}]}]});
    let models = crate::catalog::collect(chatgpt(), |_| Ok::<_, ProviderError>(page.clone())).unwrap_or_default();
    assert_eq!(models[0].id, "openai-subscription:gpt-5");
    assert_eq!(models[0].window, Some(272000));
    let caps = models[0].capabilities.clone().unwrap_or_default();
    assert!(caps.reasoning);
    assert_eq!(caps.reasoning_levels, ["low", "high"]);
}

#[test]
fn hidden_models_stay_out_of_the_catalog() {
    let page = json!({"models": [{"slug": "gpt-5", "visibility": "list"}, {"slug": "codex-auto-review", "visibility": "hide"}]});
    let models = crate::catalog::collect(chatgpt(), |_| Ok::<_, ProviderError>(page.clone())).unwrap_or_default();
    let ids: Vec<_> = models.iter().map(|model| model.id.as_str()).collect();
    assert_eq!(ids, ["openai-subscription:gpt-5"]);
}

#[test]
fn the_tokens_and_the_account_go_in_the_headers() {
    let tokens = Tokens { account_id: "acct".into(), ..Tokens::bearer("a") };
    let config = Config { auth: Some(tokens), ..Config::default() };
    let headers = config.headers(chatgpt());
    assert!(headers.contains(&("chatgpt-account-id".into(), "acct".into())));
    assert_eq!(config.base(chatgpt()), "https://chatgpt.com/backend-api");
}

#[test]
fn the_id_token_names_the_account_when_the_access_token_does_not() {
    let response = |id_token: Value| {
        json!({"access_token": jwt(json!({})), "id_token": jwt(id_token), "refresh_token": "r", "expires_in": 3600})
            .to_string()
    };
    let account = |body: String| chatgpt().tokens(200, &body, None, Timestamp::now()).map(|tokens| tokens.account_id);
    let nested = response(json!({"https://api.openai.com/auth": {"chatgpt_account_id": "acct-id"}}));
    assert_eq!(account(nested), Ok("acct-id".into()));
    assert_eq!(account(response(json!({"chatgpt_account_id": "acct-flat"}))), Ok("acct-flat".into()));
    assert_eq!(account(response(json!({"organizations": [{"id": "org-1"}]}))), Ok("org-1".into()));
    assert!(account(response(json!({}))).is_err(), "no account anywhere");
}

#[test]
fn openai_s_own_providers_keep_tools_a_search_loaded_in_the_conversation() {
    let mut body = json!({
        "model": "gpt-5.6-sol",
        "tools": [],
        "input": [{"type": "additional_tools", "role": "developer", "tools": [{"name": "a"}]}],
    });
    let mut shaped = body.clone();
    crate::request::shape(chatgpt(), None, &mut shaped).expect("shaped");
    assert_eq!(shaped["input"][0]["type"], "additional_tools");
    body["model"] = "gpt-5".into();
    crate::request::shape(chatgpt(), None, &mut body).expect("shaped");
    assert_eq!(body["tools"], json!([{"name": "a"}]), "older models get them in the list");
}

#[test]
fn both_windows_are_named_by_their_length_and_reset_at_their_epoch() {
    let value = json!({
        "plan_type": "pro",
        "rate_limit": {
            "allowed": true,
            "primary_window": {"used_percent": 42, "limit_window_seconds": 18000, "reset_after_seconds": 60, "reset_at": 1_795_456_800},
            "secondary_window": {"used_percent": 10.5, "limit_window_seconds": 604_800, "reset_at": 1_795_888_800}
        }
    });
    let usage = chatgpt().windows(&value, Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(usage.plan.as_deref(), Some("Pro"));
    assert_eq!(
        shown(&usage),
        [
            ("primary", "Sessão (5 h)", 42.0, Some("2026-11-23T18:00:00Z".to_owned())),
            ("secondary", "Semana", 10.5, Some("2026-11-28T18:00:00Z".to_owned())),
        ]
    );
}

#[test]
fn a_missing_window_is_skipped_and_a_relative_reset_still_gives_an_instant() {
    let value = json!({"plan_type": null, "rate_limit": {
        "primary_window": {"used_percent": 5, "limit_window_seconds": 10_800, "reset_after_seconds": 30},
        "secondary_window": null
    }});
    let usage = chatgpt().windows(&value, Language::Portuguese).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(usage.plan, None);
    assert_eq!(usage.windows.len(), 1);
    assert_eq!(usage.windows[0].label, "3 h");
    assert!(usage.windows[0].resets_at.is_some());
}

#[test]
fn the_call_carries_the_account_and_asks_for_json() {
    let tokens = Tokens { account_id: "acct".into(), ..Tokens::bearer("a") };
    let call = chatgpt().limits(&Config { auth: Some(tokens), ..Config::default() });
    let call = call.unwrap_or_else(|| panic!("limits"));
    assert_eq!(call.url, "https://chatgpt.com/backend-api/wham/usage");
    assert!(call.headers.contains(&("chatgpt-account-id".to_owned(), "acct".to_owned())));
    assert!(call.headers.contains(&("accept".to_owned(), "application/json".to_owned())));
}

#[test]
fn a_reasoning_family_without_listed_levels_offers_the_four_it_accepts() {
    let page = json!({"models": [{"slug": "gpt-5.5"}, {"slug": "gpt-4o"}, {"slug": "codex-x"}]});
    let models = crate::catalog::collect(chatgpt(), |_| Ok::<_, ProviderError>(page.clone())).unwrap_or_default();
    let offered: Vec<Vec<&str>> = models.iter().map(crate::efforts).collect();
    assert_eq!(offered, [vec!["low", "medium", "high", "xhigh"], vec![], vec![]]);
}
