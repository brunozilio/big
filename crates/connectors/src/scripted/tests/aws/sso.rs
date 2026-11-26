use domain::Language;
use serde_json::json;

use crate::descriptor::AuthSpec;
use crate::http::{FakeHttp, Method};
use crate::oauth::{Grant, Polled, Stepped};
use crate::provider::Provider;
use crate::scripted::tests::{loaded, now};

const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

fn sso() -> Stepped {
    let alternatives = loaded("aws").alternatives(Language::Portuguese);
    let Some(AuthSpec::Steps(sign_in)) = alternatives.first().map(|alternative| alternative.auth.clone()) else {
        panic!("not steps")
    };
    sign_in
}

#[test]
fn the_console_sign_in_is_the_default_and_the_company_sso_the_alternative() {
    let provider = loaded("aws");
    let AuthSpec::Loopback(_) = provider.descriptor().auth else { panic!("not loopback") };
    let alternatives = provider.alternatives(Language::Portuguese);
    assert_eq!(alternatives[0].key, "sso");
    assert_eq!(alternatives[0].label, "Usar SSO da empresa");
    assert_eq!(alternatives[0].button, "Continuar");
    assert_eq!(alternatives.get(1).map(|keys| keys.key.as_str()), Some("access_keys"));
    let fields = sso().0.fields(Language::Portuguese);
    let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
    assert_eq!(names, ["start_url", "region"]);
    assert_eq!(fields[0].placeholder, "https://empresa.awsapps.com/start");
    assert_eq!(fields[1].value, "us-east-1");
}

#[tokio::test]
async fn a_start_url_is_required() {
    let refused = sso().0.start(&FakeHttp::new(), &json!({ "start_url": " " }), Language::Portuguese).await;
    assert!(refused.is_err_and(|error| error.message.contains("Informe a Start URL do IAM Identity Center.")));
}

#[tokio::test]
async fn a_dynamic_client_is_registered_and_the_device_code_polled_in_the_region() {
    let http = FakeHttp::new()
        .once(Method::Post, "/client/register", 200, r#"{"clientId":"c1","clientSecret":"s1"}"#)
        .once(
            Method::Post,
            "/device_authorization",
            200,
            r#"{"deviceCode":"d","userCode":"U","verificationUri":"https://v","verificationUriComplete":"https://v?c=U","expiresIn":600,"interval":2}"#,
        )
        .once(Method::Post, "/token", 400, r#"{"error":"authorization_pending"}"#)
        .once(Method::Post, "/token", 400, r#"{"error":"slow_down","interval":10}"#)
        .once(Method::Post, "/token", 400, r#"{"error":"expired_token"}"#)
        .once(Method::Post, "/token", 400, r#"{"error":"bad_client"}"#)
        .once(Method::Post, "/token", 200, r#"{"accessToken":"at","refreshToken":"rt","expiresIn":3600}"#);
    let fields = json!({ "start_url": "https://portal.example/start", "region": "eu-west-1" });
    let started = sso().0.start(&http, &fields, Language::Portuguese).await;
    let Ok(started) = started else { panic!("{started:?}") };
    assert_eq!(started.code.verification_uri_complete.as_deref(), Some("https://v?c=U"));
    assert_eq!((started.code.user_code.as_str(), started.code.interval, started.code.expires_in), ("U", 2, 600));
    let sent = http.sent();
    assert_eq!(sent[0].url, "https://oidc.eu-west-1.amazonaws.com/client/register");
    assert_eq!(sent[0].body_json()["clientName"], brand::aws::CLIENT_NAME);
    assert_eq!(sent[0].body_json()["scopes"], json!(["sso:account:access"]));
    let start = sent[1].body_json();
    assert_eq!((start["clientId"].clone(), start["clientSecret"].clone()), (json!("c1"), json!("s1")));
    assert_eq!(start["startUrl"], "https://portal.example/start");
    let sign_in = sso();
    assert_eq!(sign_in.0.poll(&http, &started, now()).await, Ok(Polled::Pending));
    assert_eq!(sign_in.0.poll(&http, &started, now()).await, Ok(Polled::SlowDown { interval: Some(10) }));
    assert_eq!(sign_in.0.poll(&http, &started, now()).await, Ok(Polled::Expired));
    let refused = sign_in.0.poll(&http, &started, now()).await;
    assert_eq!(refused.map_err(|error| error.class), Err(crate::error::ErrorClass::Auth));
    let polled = sign_in.0.poll(&http, &started, now()).await;
    let Ok(Polled::Authorized(grant)) = polled else { panic!("{polled:?}") };
    assert_eq!(grant.access_token, "at");
    assert_eq!(grant.client.map(|client| client.client_id), Some("c1".to_owned()));
    assert_eq!(grant.extra["region"], "eu-west-1");
    assert_eq!(grant.expires_at.map(|at| at.as_second()), Some(4_600));
    let token = http.sent()[2].body_json();
    assert_eq!(token["grantType"], DEVICE_GRANT);
    assert_eq!(token["deviceCode"], "d");
    assert_eq!(http.sent()[2].url, "https://oidc.eu-west-1.amazonaws.com/token");
}

#[tokio::test]
async fn a_refresh_uses_the_registered_client_in_the_region_and_keeps_the_refresh_token() {
    let http = FakeHttp::new().once(Method::Post, "/token", 200, r#"{"accessToken":"at2","expiresIn":3600}"#);
    let mut grant = Grant::bearer("at");
    grant.refresh_token = Some("rt".to_owned());
    grant.client = Some(crate::oauth::RegisteredClient { client_id: "c1".into(), client_secret: "s1".into() });
    grant.extra = json!({ "region": "eu-west-1" });
    let fresh = sso().0.refresh(&http, &grant, now()).await;
    let Ok(fresh) = fresh else { panic!("{fresh:?}") };
    assert_eq!((fresh.access_token.as_str(), fresh.refresh_token.as_deref()), ("at2", Some("rt")));
    assert_eq!(fresh.extra["region"], "eu-west-1");
    let sent = &http.sent()[0];
    assert_eq!(sent.url, "https://oidc.eu-west-1.amazonaws.com/token");
    assert_eq!(
        sent.body_json(),
        json!({ "clientId": "c1", "clientSecret": "s1", "grantType": "refresh_token", "refreshToken": "rt" })
    );
    let lost = sso().0.refresh(&http, &Grant::bearer("at"), now()).await;
    assert!(lost.is_err_and(|error| error.message.contains("the registered client is missing")));
}
