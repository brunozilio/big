use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use app::{ModelProvider, ProviderSummary, SignedIn};
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult};
use crate::sign_in::{Stage, Waiting};
use crate::state::{Shared, held};

pub const PREFIX: &str = "ai:";
pub const GROUP: &str = "Provedores de IA";
const FIRST_STEP: Duration = Duration::from_secs(20);

#[derive(Debug, Default)]
struct Progress {
    authorize: Option<(String, Option<String>)>,
    ended: Option<Result<Value, String>>,
}

#[derive(Debug, Clone, Default)]
pub struct AiFlow {
    progress: Arc<Mutex<Progress>>,
}

pub fn subscriptions(shared: &Shared) -> Vec<ProviderSummary> {
    let listed = shared.backend.client.providers().unwrap_or_default();
    listed.into_iter().filter(|summary| summary.access.subscription()).collect()
}

pub fn provider_json(summary: &ProviderSummary) -> Value {
    let sign_in = match summary.access {
        domain::provider::Access::Code => "device",
        _ => "browser",
    };
    let name = if summary.label.is_empty() { summary.name.as_str() } else { summary.label.as_str() };
    let logo = if summary.logo.is_empty() { summary.provider.id() } else { summary.logo.trim() };
    json!({
        "key": summary.provider.id(), "name": name, "group": GROUP,
        "description": format!("Sua conta {name}"), "logo": logo, "button": format!("Entrar com {name}"),
        "site": format!("do {name}"), "signIn": sign_in, "fields": [], "multiple": true, "alternative": null, "ai": true,
    })
}

pub fn connections_of(summary: &ProviderSummary) -> Vec<Value> {
    let provider = summary.provider.id();
    let mut accounts = summary.accounts.clone();
    if accounts.is_empty() {
        accounts.extend(summary.account.clone());
    }
    accounts
        .iter()
        .map(|account| {
            let key = if account.id.is_empty() { &account.label } else { &account.id };
            json!({
                "id": format!("{PREFIX}{provider}:{key}"), "provider": provider,
                "label": if summary.label.is_empty() { &summary.name } else { &summary.label },
                "account": account.label, "capabilities": ["ai"], "status": "ok",
            })
        })
        .collect()
}

fn known(shared: &Shared, provider: &str) -> ApiResult<ProviderSummary> {
    let found = subscriptions(shared).into_iter().find(|summary| summary.provider.id() == provider);
    found.ok_or_else(|| ApiError::missing("Não há esse provedor."))
}

fn connected(provider: &str, listed: &[ProviderSummary]) -> Result<Value, String> {
    let summary = listed.iter().find(|summary| summary.provider.id() == provider);
    let summary = summary.ok_or_else(|| "O provedor não ficou conectado.".to_owned())?;
    connections_of(summary).into_iter().last().ok_or_else(|| "O provedor não ficou conectado.".to_owned())
}

fn run(client: &app::Client, provider: ModelProvider, back: &str, flow: &AiFlow) {
    let progress = Arc::clone(&flow.progress);
    let signed = client.sign_in_returning(provider, back, |url, code| {
        held(&progress).authorize = Some((url.to_owned(), code.map(str::to_owned)));
    });
    let ended = match signed {
        Ok(SignedIn::Connected(listed)) => connected(provider.id(), &listed),
        Ok(SignedIn::PortBusy { port, .. }) => Err(format!("A porta {port} do login está ocupada por outro programa.")),
        Err(error) => Err(error.to_string()),
    };
    held(&flow.progress).ended = Some(ended);
}

fn is_local(host: &str) -> bool {
    let name = host.rsplit_once(':').map_or(host, |(name, port)| if port.ends_with(']') { host } else { name });
    matches!(name, "localhost" | "127.0.0.1" | "[::1]")
}

fn origin(shared: &Shared, host: Option<&str>) -> String {
    match host.filter(|host| is_local(host)) {
        Some(host) => format!("http://{host}"),
        None => format!("http://127.0.0.1:{}", shared.port),
    }
}

pub fn start(shared: &Shared, provider: &str, host: Option<&str>) -> ApiResult<Value> {
    let summary = known(shared, provider)?;
    let client = shared.backend.client.sibling().map_err(ApiError::failed)?;
    let flow = AiFlow::default();
    let id = shared.flows.open(provider, Waiting::Ai(flow.clone()))?;
    let back = format!("{}{}", origin(shared, host), crate::sign_in::back_to(provider, &id));
    let running = flow.clone();
    let chosen = summary.provider;
    std::thread::Builder::new()
        .name("ai-sign-in".into())
        .spawn(move || run(&client, chosen, &back, &running))
        .map_err(ApiError::failed)?;
    let began = Instant::now();
    while began.elapsed() < FIRST_STEP {
        let (authorize, ended) = {
            let progress = held(&flow.progress);
            (progress.authorize.clone(), progress.ended.clone())
        };
        if let Some(Err(message)) = ended {
            return Err(ApiError::upstream(message));
        }
        if let Some((url, code)) = authorize {
            return Ok(match code {
                Some(code) => {
                    json!({ "flow": id, "kind": "device", "url": url, "code": code, "expiresIn": 900, "interval": 2 })
                }
                None => json!({ "flow": id, "kind": "redirect", "url": url, "paste": false, "returns": true }),
            });
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(ApiError::upstream("O provedor não respondeu a tempo."))
}

pub fn poll(shared: &Shared, flow: &AiFlow) -> Stage {
    let ended = held(&flow.progress).ended.clone();
    match ended {
        None => Stage::Waiting(Waiting::Ai(flow.clone())),
        Some(Ok(connection)) => {
            crate::connections::announce(shared);
            Stage::Done(connection)
        }
        Some(Err(message)) => Stage::Failed(message),
    }
}

pub fn disconnect(shared: &Shared, id: &str) -> ApiResult<()> {
    let rest = id.strip_prefix(PREFIX).ok_or_else(|| ApiError::missing("Não há essa conexão."))?;
    let (provider, account) = rest.split_once(':').unwrap_or((rest, ""));
    let summary = known(shared, provider)?;
    let client = &shared.backend.client;
    let several = summary.accounts.len() > 1;
    let removed = if several && !account.is_empty() {
        client.remove_account(summary.provider, account)
    } else {
        client.disconnect_provider(summary.provider)
    };
    removed.map(|_| ()).map_err(ApiError::failed)
}

#[cfg(test)]
mod tests {
    use super::is_local;

    #[test]
    fn only_the_local_machine_is_a_place_to_come_back_to() {
        assert!(is_local("127.0.0.1:7999"));
        assert!(is_local("localhost:5173"));
        assert!(is_local("localhost"));
        assert!(is_local("[::1]:7999"));
        assert!(!is_local("evil.test:7999"));
        assert!(!is_local("localhost.evil.test"));
    }
}
