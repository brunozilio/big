mod ledger;

use std::sync::{LazyLock, Mutex, PoisonError};

use async_trait::async_trait;
use domain::Language;
use domain::connection::{Capability, ProviderKey};
use jiff::Timestamp;
use serde_json::{Value, json};

use crate::contracts::{self, changes as actions};
use crate::descriptor::{AuthSpec, Presentation, ProviderDescriptor, TokenField, TokenForm, TokenSpec};
use crate::error::ActionError;
use crate::http::Http;
use crate::provider::{
    AccountOption, Args, Credentials, EventBatch, EventSource, ExternalEvent, Provider, unknown_action,
};
pub use ledger::{AWAITING, CANCELLED, DONE, EXECUTING, Ledger, Pace};

pub const KEY: &str = "mock-changes";
const LOGO: &str = include_str!("logo.svg");

static LEDGER: LazyLock<Mutex<Ledger>> = LazyLock::new(Mutex::default);

#[derive(Debug, Clone, Copy, Default)]
pub struct MockChanges;

#[derive(Debug, Clone, Copy, Default)]
pub struct MockChangeEvents;

fn token_spec(language: Language) -> TokenSpec {
    let label = language.pick("Name", "Nombre", "Nome");
    let help = language.pick(
        "Any name: this simulated system keeps the changes in memory.",
        "Cualquier nombre: este sistema simulado guarda los cambios en memoria.",
        "Qualquer nome: este sistema simulado guarda as mudanças na memória.",
    );
    TokenSpec { fields: vec![TokenField::new("name", label, help, false)], form: TokenForm::Bearer }
}

pub fn run(action: &str, args: &Value, key: &str, ledger: &mut Ledger, pace: Pace, now: i64) -> Result<Value, ActionError> {
    let args = Args(args);
    match action {
        actions::CREATE => ledger.create(args, key, pace, now),
        actions::UPDATE => ledger.update(args, pace, now),
        actions::GET => ledger.get(args, pace, now),
        actions::CANCEL => ledger.cancel(args, pace, now),
        _ => Err(unknown_action(action)),
    }
}

pub fn events_between(ledger: &Ledger, creds: &Credentials, pace: Pace, since: i64, now: i64) -> Vec<ExternalEvent> {
    ledger
        .moved(pace, since, now)
        .into_iter()
        .map(|(reference, status, at)| ExternalEvent {
            id: format!("{reference}:{status}"),
            connection: creds.connection.clone(),
            capability: Capability::from(contracts::CHANGES),
            kind: actions::STATUS_CHANGED.to_owned(),
            subject: reference.clone(),
            data: json!({ "ref": reference, "status": status }),
            at: Timestamp::from_second(at).unwrap_or(Timestamp::UNIX_EPOCH),
        })
        .collect()
}

#[async_trait]
impl EventSource for MockChangeEvents {
    async fn poll(
        &self,
        _http: &dyn Http,
        creds: &Credentials,
        cursor: Option<&str>,
    ) -> Result<EventBatch, ActionError> {
        let now = Timestamp::now().as_second();
        let since = cursor.and_then(|text| text.parse().ok());
        let events = match since {
            Some(since) => {
                let ledger = LEDGER.lock().unwrap_or_else(PoisonError::into_inner);
                events_between(&ledger, creds, Pace::configured(), since, now)
            }
            None => Vec::new(),
        };
        Ok(EventBatch { events, cursor: now.to_string() })
    }
}

#[async_trait]
impl Provider for MockChanges {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "Change requests (mock)".to_owned(),
            logo: LOGO.to_owned(),
            capabilities: vec![Capability::from(contracts::CHANGES)],
            auth: AuthSpec::Token(token_spec(Language::default())),
            actions: contracts::changes(),
        }
    }

    fn presentation(&self, language: Language) -> Presentation {
        Presentation {
            description: language
                .pick("Simulated change requests", "Cambios simulados", "Mudanças simuladas")
                .to_owned(),
            group: contracts::CHANGES.to_owned(),
            button: crate::texts::connect(language).to_owned(),
            site: language.pick("the simulated system", "del sistema simulado", "do sistema simulado").to_owned(),
            choose_title: String::new(),
            hidden: false,
        }
    }

    async fn call(
        &self,
        _http: &dyn Http,
        _creds: &Credentials,
        action: &str,
        args: &Value,
        idempotency_key: &str,
    ) -> Result<Value, ActionError> {
        let mut ledger = LEDGER.lock().unwrap_or_else(PoisonError::into_inner);
        run(action, args, idempotency_key, &mut ledger, Pace::configured(), Timestamp::now().as_second())
    }

    async fn accounts(&self, _http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        let label = if creds.access_token.trim().is_empty() { KEY.to_owned() } else { creds.access_token.clone() };
        Ok(vec![AccountOption { id: KEY.to_owned(), label, settings: json!({}) }])
    }

    fn events(&self) -> Option<&dyn EventSource> {
        Some(&MockChangeEvents)
    }
}

#[cfg(test)]
mod tests;
