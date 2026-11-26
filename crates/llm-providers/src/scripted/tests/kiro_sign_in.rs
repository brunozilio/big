use std::cell::RefCell;
use std::time::Duration;

use jiff::Timestamp;
use serde_json::{Value, json};

use super::provider;
use crate::sign_in::{Host, Listener};
use crate::testing::{Log, ScriptedHost};
use crate::{Provider, ProviderError, Tokens};

const ARN: &str = "arn:aws:codewhisperer:us-east-1:111122223333:profile/ABCDEF";
const TOKEN: &str = "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/token";
const PROFILES: &str = "AmazonCodeWhispererService.ListAvailableProfiles";
const REDIRECT: &str = "http://127.0.0.1:49153/oauth/callback";
const ORGANIZATION: &str =
    "login_option=awsidc&issuer_url=https%3A%2F%2Facme.awsapps.com%2Fstart&idc_region=eu-west-1&state=r2x32";

fn kiro() -> &'static dyn Provider {
    provider("kiro-subscription")
}

#[derive(Default)]
struct Recording {
    host: ScriptedHost,
    headers: RefCell<Vec<Vec<(String, String)>>>,
}

impl Host for Recording {
    fn post(&self, url: &str, headers: &[(String, String)], body: &str) -> Result<(u16, String), ProviderError> {
        self.headers.borrow_mut().push(headers.to_vec());
        self.host.post(url, headers, body)
    }
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<(u16, String), ProviderError> {
        self.headers.borrow_mut().push(headers.to_vec());
        self.host.get(url, headers)
    }
    fn open(&self, url: String, code: Option<String>) {
        self.host.open(url, code);
    }
    fn listen(&self, port: u16, path: &str) -> Result<Box<dyn Listener>, ProviderError> {
        self.host.listen(port, path)
    }
    fn random(&self, bytes: usize) -> Result<String, ProviderError> {
        self.host.random(bytes)
    }
    fn sleep(&self, duration: Duration) {
        self.host.sleep(duration);
    }
    fn now(&self) -> Timestamp {
        self.host.now()
    }
}

struct Signed {
    tokens: Result<Tokens, ProviderError>,
    log: Log,
    headers: Vec<Vec<(String, String)>>,
}

fn signed(replies: Vec<(u16, String)>, visits: Vec<(&'static str, &'static str)>) -> Signed {
    let recording = Recording { host: ScriptedHost::new(replies, visits), ..Recording::default() };
    let tokens = kiro().sign_in(&recording).and_then(|config| config.refreshable().cloned());
    let log = std::mem::take(&mut *recording.host.log.borrow_mut());
    Signed { tokens, log, headers: recording.headers.take() }
}

fn reply(body: &str) -> (u16, String) {
    (200, body.to_owned())
}

fn organization(visit: &'static str) -> Signed {
    let replies = vec![
        reply(r#"{"clientId":"cid","clientSecret":"secret"}"#),
        reply(r#"{"accessToken":"a","refreshToken":"r","expiresIn":900}"#),
        (403, String::new()),
        reply(&format!(r#"{{"profiles":[{{"arn":"{ARN}","profileName":"Acme"}}]}}"#)),
    ];
    signed(replies, vec![("/signin/callback", visit), ("/oauth/callback", "code=org-code&state=r4x32")])
}

fn social(query: &'static str) -> Signed {
    let answer = format!(r#"{{"accessToken":"a","refreshToken":"r","profileArn":"{ARN}"}}"#);
    signed(vec![reply(&answer)], vec![("/oauth/callback", query)])
}

fn refused(visit: &'static str) -> String {
    let refused = signed(Vec::new(), vec![("/signin/callback", visit)]);
    assert!(refused.log.sent.is_empty());
    assert!(refused.log.answered[0].starts_with("failed"), "the tab is answered");
    refused.tokens.map(|_| ()).err().unwrap_or_else(|| panic!("refused")).to_string()
}

fn body(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_default()
}

#[test]
fn the_portal_gets_the_loopback_origin_and_the_challenge() {
    let signed = social("login_option=google&code=c&state=r2x32");
    assert!(signed.tokens.is_ok(), "{:?}", signed.tokens);
    let challenge = crate::oauth::challenge("r1x64");
    assert_eq!(
        signed.log.opened,
        [(
            format!(
                "https://app.kiro.dev/signin?state=r2x32&code_challenge={challenge}&code_challenge_method=S256\
                 &redirect_uri=http%3A%2F%2Flocalhost%3A49153&redirect_from=kirocli"
            ),
            None
        )]
    );
}

#[test]
fn the_code_is_exchanged_for_the_redirect_with_the_picked_sign_in() {
    let signed = social("login_option=google&code=c&state=s");
    assert!(signed.tokens.is_ok(), "{:?}", signed.tokens);
    let (url, sent) = &signed.log.sent[0];
    assert_eq!(url, TOKEN);
    assert_eq!(
        body(sent),
        json!({"code": "c", "code_verifier": "r1x64",
            "redirect_uri": "http://localhost:49153/oauth/callback?login_option=google"})
    );
}

#[test]
fn only_google_and_github_codes_go_to_kiros_auth_service() {
    let other = social("login_option=facebook&code=c");
    assert!(
        matches!(&other.tokens, Err(ProviderError::Unauthorized(why)) if why.contains("facebook")),
        "{:?}",
        other.tokens
    );
    assert!(social("code=c").tokens.is_err());
    assert!(social("login_option=github&code=c").tokens.is_ok());
}

#[test]
fn the_portal_names_an_organization_s_start_url_and_region() {
    let signed = organization(ORGANIZATION);
    assert!(signed.tokens.is_ok(), "{:?}", signed.tokens);
    assert_eq!(signed.log.sent[0].0, "https://oidc.eu-west-1.amazonaws.com/client/register");
    assert_eq!(body(&signed.log.sent[0].1)["issuerUrl"], "https://acme.awsapps.com/start");
    let google = social("login_option=google&code=c&state=s");
    assert_eq!(google.log.sent[0].0, TOKEN);
    let builder = organization("login_option=builderid&state=s");
    assert_eq!(builder.log.sent[0].0, "https://oidc.us-east-1.amazonaws.com/client/register");
    assert_eq!(body(&builder.log.sent[0].1)["issuerUrl"], "https://view.awsapps.com/start");
}

#[test]
fn an_organization_answer_without_its_start_url_or_region_says_so() {
    assert!(refused("login_option=awsidc&issuer_url=&amplitude_device_id=").contains("issuer_url"));
    assert!(refused("login_option=awsidc&issuer_url=https%3A%2F%2Fa.example").contains("region"));
    let bad = refused("login_option=awsidc&issuer_url=https%3A%2F%2Fa.example&idc_region=evil.host%2Fx");
    assert!(bad.contains("is not an AWS region"), "a region never becomes part of a host: {bad}");
    let plain = refused("login_option=awsidc&issuer_url=http%3A%2F%2Fa.example&idc_region=us-east-1");
    assert!(plain.contains("not https"), "the start URL is https: {plain}");
}

#[test]
fn the_client_registers_with_the_region_and_the_browser_authorizes_there() {
    let signed = organization(ORGANIZATION);
    assert!(signed.tokens.is_ok(), "{:?}", signed.tokens);
    let registered = body(&signed.log.sent[0].1);
    assert_eq!(registered["issuerUrl"], "https://acme.awsapps.com/start");
    assert_eq!(registered["clientName"], "Kiro CLI", "the sign-in page names Kiro");
    assert_eq!(registered["redirectUris"], json!([REDIRECT]));
    assert_eq!(registered["grantTypes"], json!(["authorization_code", "refresh_token"]));
    let challenge = crate::oauth::challenge("r3x64");
    assert_eq!(
        signed.log.answered[0],
        format!(
            "redirect https://oidc.eu-west-1.amazonaws.com/authorize?response_type=code&client_id=cid\
             &redirect_uri=http%3A%2F%2F127.0.0.1%3A49153%2Foauth%2Fcallback\
             &scopes=codewhisperer%3Acompletions+codewhisperer%3Aanalysis+codewhisperer%3Aconversations\
             &state=r4x32&code_challenge={challenge}&code_challenge_method=S256"
        )
    );
    let unregistered = signed_with_registration((400, "{}".to_owned()));
    assert!(unregistered.tokens.is_err());
    assert!(unregistered.log.answered[0].starts_with("failed"), "{:?}", unregistered.log.answered);
}

fn signed_with_registration(registration: (u16, String)) -> Signed {
    signed(vec![registration], vec![("/signin/callback", ORGANIZATION)])
}

#[test]
fn the_code_and_later_the_refresh_token_go_to_the_region_s_token_endpoint() {
    let signed = organization(ORGANIZATION);
    let fresh = signed.tokens.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(signed.log.sent[1].0, "https://oidc.eu-west-1.amazonaws.com/token");
    assert_eq!(
        body(&signed.log.sent[1].1),
        json!({"clientId": "cid", "clientSecret": "secret", "grantType": "authorization_code", "code": "org-code",
            "redirectUri": REDIRECT, "codeVerifier": "r3x64"})
    );
    assert_eq!((fresh.access_token.as_str(), fresh.refresh_token.as_str()), ("a", "r"));
    assert_eq!(fresh.expires_at, Some(Timestamp::UNIX_EPOCH + jiff::SignedDuration::from_secs(900)));
    let client = json!({"region": "eu-west-1", "client_id": "cid", "client_secret": "secret"});
    assert_eq!(fresh.client, Some(client), "kept for refreshes");
    let refresh = kiro().refresh(&fresh).unwrap_or_else(|| panic!("refreshable"));
    assert_eq!(refresh.url, "https://oidc.eu-west-1.amazonaws.com/token");
    assert_eq!(body(&refresh.body)["grantType"], "refresh_token");
    assert_eq!(body(&refresh.body)["refreshToken"], "r");
    let now = Timestamp::now();
    let renewed = kiro().tokens(200, r#"{"accessToken":"b"}"#, Some(&fresh), now).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((renewed.refresh_token.as_str(), renewed.account_id.as_str()), ("r", ARN), "kept from before");
    assert!(renewed.client.is_some());
    assert!(kiro().tokens(400, "{}", Some(&fresh), now).is_err());
}

#[test]
fn the_profile_comes_from_the_first_listed() {
    let signed = organization(ORGANIZATION);
    assert_eq!(signed.tokens.map(|tokens| tokens.account_id), Ok(ARN.to_owned()));
    let urls: Vec<_> = signed.log.sent.iter().map(|(url, _)| url.as_str()).collect();
    assert_eq!(&urls[2..], ["https://q.us-east-1.amazonaws.com/", "https://q.eu-central-1.amazonaws.com/"]);
    let asked = &signed.headers[3];
    assert!(asked.contains(&("x-amz-target".into(), PROFILES.into())));
    assert!(asked.contains(&("authorization".into(), "Bearer a".into())));
    let empty = signed_with_profiles([(200, r#"{"profiles":[]}"#.to_owned()), (403, String::new())]);
    assert!(empty.to_string().contains("no Kiro profile"), "{empty}");
}

fn signed_with_profiles(profiles: [(u16, String); 2]) -> ProviderError {
    let mut replies = vec![
        reply(r#"{"clientId":"cid","clientSecret":"secret"}"#),
        reply(r#"{"accessToken":"a","refreshToken":"r"}"#),
    ];
    replies.extend(profiles);
    let signed = signed(replies, vec![("/signin/callback", ORGANIZATION), ("/oauth/callback", "code=c&state=r4x32")]);
    signed.tokens.map(|_| ()).err().unwrap_or_else(|| panic!("refused"))
}

#[test]
fn an_organization_sign_in_goes_on_to_its_identity_center_in_the_same_tab() {
    let signed = organization(ORGANIZATION);
    let tokens = signed.tokens.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((tokens.access_token.as_str(), tokens.account_id.as_str()), ("a", ARN));
    assert!(tokens.client.is_some(), "kept for refreshes");
    let log = signed.log;
    assert!(log.opened[0].0.starts_with("https://app.kiro.dev/signin?"), "{:?}", log.opened);
    assert!(
        log.answered[0].starts_with("redirect https://oidc.eu-west-1.amazonaws.com/authorize?"),
        "{:?}",
        log.answered
    );
    assert_eq!(log.answered[1], "done");
    let urls: Vec<_> = log.sent.iter().map(|(url, _)| url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://oidc.eu-west-1.amazonaws.com/client/register",
            "https://oidc.eu-west-1.amazonaws.com/token",
            "https://q.us-east-1.amazonaws.com/",
            "https://q.eu-central-1.amazonaws.com/",
        ]
    );
    assert!(log.sent[1].1.contains("\"code\":\"org-code\""), "{}", log.sent[1].1);
}

#[test]
fn an_organization_answer_without_its_start_url_fails_in_the_tab_and_the_terminal() {
    let error = refused("login_option=awsidc&issuer_url=&amplitude_device_id=&state=r2x32");
    assert!(error.contains("issuer_url"), "{error}");
}

#[test]
fn an_account_without_a_profile_is_told_so() {
    let replies = vec![
        reply(r#"{"clientId":"cid","clientSecret":"secret"}"#),
        reply(r#"{"accessToken":"a","refreshToken":"r"}"#),
        (403, String::new()),
        reply(r#"{"profiles":[]}"#),
    ];
    let visits =
        vec![("/signin/callback", "login_option=builderid&state=r2x32"), ("/oauth/callback", "code=c&state=r4x32")];
    let error = signed(replies, visits).tokens.map(|_| ()).err().unwrap_or_else(|| panic!("refused"));
    assert!(error.to_string().contains("no Kiro profile"), "{error}");
}

#[test]
fn a_social_sign_in_exchanges_its_code_with_kiro() {
    let signed = social("login_option=google&code=c&state=r2x32");
    assert_eq!(signed.tokens.map(|tokens| tokens.account_id), Ok(ARN.to_owned()));
    assert_eq!(signed.log.sent[0].0, TOKEN);
}
