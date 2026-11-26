use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

use agent_engine::model::client::ProviderClient;
use agent_engine::model::http::{Body, Http, Response};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use domain::provider::{ProviderKey, ProviderKeyStore};
use domain::{ModelProvider, SecretId};
use serde_json::{Value, json};

use super::{AccountStore, CONFIG};
use crate::providers::{InMemoryProviderStore, ProviderStore};
use crate::secrets::{Secrets, secrets_in};

const CHATGPT: ModelProvider = ModelProvider::new("openai-subscription");
const ANSWER_DELAY: Duration = Duration::from_millis(300);

fn access_token() -> String {
    let claims = json!({"https://api.openai.com/auth": {"chatgpt_account_id": "account-1"}});
    format!("e30.{}.sig", URL_SAFE_NO_PAD.encode(claims.to_string()))
}

fn answer(status: u16, value: &Value) -> Response {
    Response { status, headers: Vec::new(), body: Body::from(value.to_string()) }
}

#[derive(Debug)]
struct SingleUseRefreshTokens {
    valid: Mutex<String>,
    issued: AtomicUsize,
    refused: AtomicUsize,
}

impl Http for SingleUseRefreshTokens {
    fn post(&self, _url: &str, _headers: &[(String, String)], body: &str) -> Result<Response, String> {
        let presented =
            body.split('&').find_map(|pair| pair.strip_prefix("refresh_token=")).unwrap_or_default().to_owned();
        std::thread::sleep(ANSWER_DELAY);
        let mut valid = self.valid.lock().expect("valid token");
        if presented != *valid {
            self.refused.fetch_add(1, Ordering::SeqCst);
            return Ok(answer(400, &json!({"error": "invalid_grant"})));
        }
        let next = self.issued.fetch_add(1, Ordering::SeqCst) + 2;
        *valid = format!("refresh-{next}");
        Ok(answer(200, &json!({"access_token": access_token(), "refresh_token": *valid, "expires_in": 3600})))
    }

    fn get(&self, _url: &str, _headers: &[(String, String)]) -> Result<Response, String> {
        Ok(answer(200, &json!({"models": []})))
    }
}

#[test]
fn two_processes_with_a_stale_sign_in_refresh_it_once_and_both_stay_signed_in() {
    let dir = tempfile::tempdir().expect("dir");
    let slot = SecretId::new();
    let expired = json!({"auth": {"access_token": access_token(), "refresh_token": "refresh-1", "account_id": "account-1", "expires_at": "2000-01-01T00:00:00Z"}});
    secrets_in(Some(dir.path()))
        .set(slot, &Secrets::from([(CONFIG.to_owned(), expired.to_string())]))
        .expect("signed in");
    let endpoint = Arc::new(SingleUseRefreshTokens {
        valid: Mutex::new("refresh-1".to_owned()),
        issued: AtomicUsize::new(0),
        refused: AtomicUsize::new(0),
    });
    let start = Arc::new(Barrier::new(2));

    let processes: Vec<_> = (0..2)
        .map(|_| {
            let vault = secrets_in(Some(dir.path()));
            let (endpoint, start) = (Arc::clone(&endpoint), Arc::clone(&start));
            std::thread::spawn(move || {
                let book = Arc::new(InMemoryProviderStore::default());
                let source =
                    Arc::new(AccountStore { secrets: vault, slot, provider: CHATGPT, book, account: String::new() });
                let key = ProviderKey::new(source.load().expect("load")).with_store(source);
                start.wait();
                ProviderClient::new(CHATGPT, endpoint, Arc::new(key)).models().map(|_| ())
            })
        })
        .collect();
    let results: Vec<_> = processes.into_iter().map(|process| process.join().expect("process")).collect();

    assert_eq!(results, [Ok(()), Ok(())], "neither process was signed out");
    assert_eq!(endpoint.refused.load(Ordering::SeqCst), 0, "no refresh token was presented twice");
    assert_eq!(endpoint.issued.load(Ordering::SeqCst), 1, "the second process used the first one's new token");
    let kept = secrets_in(Some(dir.path())).get(slot).expect("kept")[CONFIG].clone();
    assert!(kept.contains("refresh-2"), "{kept}");
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .expect("dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("refresh"))
        .collect();
    assert!(leftovers.is_empty(), "the refresh lock leaves nothing behind: {leftovers:?}");
}

#[derive(Debug)]
struct RevokedRefreshToken;

impl Http for RevokedRefreshToken {
    fn post(&self, _url: &str, _headers: &[(String, String)], _body: &str) -> Result<Response, String> {
        Ok(answer(400, &json!({"error": "invalid_grant"})))
    }

    fn get(&self, _url: &str, _headers: &[(String, String)]) -> Result<Response, String> {
        Ok(answer(200, &json!({"models": []})))
    }
}

#[test]
fn a_refused_refresh_marks_the_account_lapsed_and_signing_in_again_clears_it() {
    let dir = tempfile::tempdir().expect("dir");
    let slot = SecretId::new();
    let expired = json!({"auth": {"access_token": access_token(), "refresh_token": "dead", "account_id": "account-1", "expires_at": "2000-01-01T00:00:00Z"}});
    let vault = secrets_in(Some(dir.path()));
    vault.set(slot, &Secrets::from([(CONFIG.to_owned(), expired.to_string())])).expect("signed in");
    let book = Arc::new(InMemoryProviderStore::default());
    let account =
        domain::provider::ProviderAccount { label: "a@x.io".into(), since_unix: 1, id: "id".into(), lapsed: false };
    book.update(&mut |kept| kept.connect(CHATGPT, account.clone())).expect("book");
    let source =
        Arc::new(AccountStore { secrets: vault, slot, provider: CHATGPT, book: book.clone(), account: "id".into() });
    let key = ProviderKey::new(source.load().expect("load")).with_store(source);

    assert!(ProviderClient::new(CHATGPT, Arc::new(RevokedRefreshToken), Arc::new(key)).models().is_err());

    assert!(book.load().accounts(CHATGPT)[0].lapsed, "the account asks to reconnect");
    book.update(&mut |kept| kept.connect(CHATGPT, account.clone())).expect("signed in again");
    assert!(!book.load().accounts(CHATGPT)[0].lapsed);
}
