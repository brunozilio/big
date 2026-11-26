use serde_json::json;

use super::super::*;
use crate::testing::ScriptedHost;

fn loaded(name: &str) -> ScriptedProvider {
    ScriptedProvider::load(&format!("{FOLDER}{name}/{ENTRY}")).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn grok_loads_and_answers_through_its_script() {
    let grok = loaded("grok");
    assert_eq!(
        (grok.id(), grok.wire(None), grok.path(None), grok.rank()),
        ("grok-subscription", "responses", "/responses", 60)
    );
    assert!(grok.logo().starts_with("<svg"));
    let tokens = Tokens { refresh_token: "r".into(), ..Tokens::bearer("a") };
    let config = Config { auth: Some(tokens.clone()), ..Config::default() };
    assert!(config.validate(&grok).is_ok());
    assert_eq!(config.base(&grok), "https://api.x.ai/v1");
    assert!(grok.headers(&config).contains(&("authorization".into(), "Bearer a".into())));
    let refresh = grok.refresh(&tokens).unwrap_or_else(|| panic!("refresh"));
    assert!(refresh.body.contains("refresh_token=r"), "{}", refresh.body);
    let page = json!({"data": [{"id": "grok-4.6", "context_windows": [256000, 500000]}, {"id": "grok-imagine"}]});
    let models = crate::catalog::collect(&grok, |_| Ok::<_, ProviderError>(page.clone())).unwrap_or_default();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].window, Some(500000));
}

#[test]
fn grok_signs_in_with_a_device_code() {
    let code = json!({"device_code": "dc", "user_code": "AB-12", "verification_uri_complete": "https://accounts.x.ai/device?code=AB-12", "interval": 5, "expires_in": 600});
    let host = ScriptedHost::new(
        [
            (200, code.to_string()),
            (400, json!({"error": "slow_down"}).to_string()),
            (200, json!({"access_token": "at", "refresh_token": "rt", "expires_in": 21600}).to_string()),
        ],
        [],
    );
    let config = loaded("grok").sign_in(&host).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(config.access_token(), "at");
    let log = host.log.borrow();
    assert_eq!(log.waits.iter().map(std::time::Duration::as_secs).collect::<Vec<_>>(), [5, 10]);
}

#[test]
fn a_provider_runs_here_only_when_the_build_offers_all_it_needs() {
    let mut grok = loaded("grok");
    assert!(grok.runs_here());
    grok.needs = vec!["local".to_owned()];
    assert_eq!(grok.runs_here(), cfg!(feature = "local-models"));
}
