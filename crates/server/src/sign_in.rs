use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use connectors::{
    AccountOption, AuthSpec, Authorization, DeviceApp, DeviceCode, Grant, Loopback, OAuthApp, Polled,
    ProviderDescriptor,
};
use domain::connection::{Capability, Connection, ConnectionId, ProviderKey};
use serde_json::{Value, json};
use store::ConnectionRecord;

use crate::ai::AiFlow;
use crate::attachments::random_id;
use crate::error::{ApiError, ApiResult};
use crate::state::{Shared, held};

const FLOW_LASTS: Duration = Duration::from_secs(1800);

#[derive(Debug, Clone)]
pub enum Exchanger {
    App(OAuthApp),
    Loopback(Loopback),
}

#[derive(Debug, Clone)]
pub enum Waiting {
    Redirect { exchanger: Exchanger, authorization: Authorization },
    Device { app: DeviceApp, code: DeviceCode, client: Option<connectors::RegisteredClient> },
    Ai(AiFlow),
}

#[derive(Debug, Clone)]
pub enum Stage {
    Waiting(Waiting),
    Granted(Grant),
    Choose { grant: Grant, options: Vec<AccountOption> },
    Done(Value),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Flow {
    pub provider: String,
    pub stage: Stage,
    pub started: Instant,
    pub method: Option<String>,
    pub fields: Value,
}

#[derive(Debug, Default)]
pub struct Flows(Mutex<HashMap<String, Flow>>);

impl Flows {
    pub fn open(&self, provider: &str, waiting: Waiting) -> ApiResult<String> {
        self.open_with(provider, waiting, None, Value::Null)
    }

    pub fn open_with(
        &self,
        provider: &str,
        waiting: Waiting,
        method: Option<&str>,
        fields: Value,
    ) -> ApiResult<String> {
        let id = random_id()?;
        let mut flows = held(&self.0);
        flows.retain(|_, flow| flow.started.elapsed() < FLOW_LASTS);
        let flow = Flow {
            provider: provider.to_owned(),
            stage: Stage::Waiting(waiting),
            started: Instant::now(),
            method: method.map(str::to_owned),
            fields,
        };
        flows.insert(id.clone(), flow);
        Ok(id)
    }

    pub fn get(&self, id: &str) -> Option<Flow> {
        held(&self.0).get(id).cloned()
    }

    pub fn set(&self, id: &str, stage: Stage) {
        if let Some(flow) = held(&self.0).get_mut(id) {
            flow.stage = stage;
        }
    }

    pub fn by_state(&self, state: &str) -> Option<String> {
        let flows = held(&self.0);
        flows
            .iter()
            .find(|(_, flow)| {
                matches!(&flow.stage, Stage::Waiting(Waiting::Redirect { authorization, .. }) if authorization.state == state)
            })
            .map(|(id, _)| id.clone())
    }
}

fn descriptor(shared: &Shared, provider: &str) -> ApiResult<ProviderDescriptor> {
    let key = ProviderKey::from(provider);
    shared.backend.connectors.descriptor(&key).cloned().map_err(|_| ApiError::missing("Não há esse provedor."))
}

fn chosen_auth(shared: &Shared, found: &ProviderDescriptor, method: Option<&str>) -> ApiResult<AuthSpec> {
    let Some(method) = method.filter(|method| !method.is_empty()) else { return Ok(found.auth.clone()) };
    let alternative = shared.backend.connectors.alternative(&found.key).ok().flatten();
    let alternative = alternative.filter(|alternative| alternative.key == method);
    alternative.map(|alternative| alternative.auth.clone()).ok_or_else(|| ApiError::bad("Não há essa forma de entrar."))
}

fn missing_client(found: &ProviderDescriptor, auth: &AuthSpec) -> Option<String> {
    let client_id = match auth {
        AuthSpec::Redirect(app) => app.client_id.as_str(),
        AuthSpec::Device(app) => app.client_id.as_str(),
        AuthSpec::Sso(_) | AuthSpec::Loopback(_) => return None,
    };
    client_id
        .trim()
        .is_empty()
        .then(|| format!("Falta o client id do {}: preencha a seção [connectors] do config.toml.", found.name))
}

pub async fn start(shared: &Shared, provider: &str, fields: &Value, method: Option<&str>) -> ApiResult<Value> {
    let found = descriptor(shared, provider)?;
    let auth = chosen_auth(shared, &found, method)?;
    if let Some(told) = missing_client(&found, &auth) {
        return Err(ApiError::bad(told));
    }
    let http = shared.backend.connectors.http();
    let opened = |waiting: Waiting| shared.flows.open_with(provider, waiting, method, fields.clone());
    let redirect = |exchanger: Exchanger, authorization: Authorization| -> ApiResult<Value> {
        let url = authorization.url.clone();
        let flow = opened(Waiting::Redirect { exchanger, authorization })?;
        Ok(json!({ "flow": flow, "kind": "redirect", "url": url, "paste": false }))
    };
    match &auth {
        AuthSpec::Redirect(app) => redirect(Exchanger::App(app.clone()), app.begin(shared.port)?),
        AuthSpec::Loopback(sign_in) => {
            let authorization = sign_in.0.begin(http, shared.port).await?;
            redirect(Exchanger::Loopback(sign_in.clone()), authorization)
        }
        AuthSpec::Device(app) => device(shared, app.clone(), None, &opened).await,
        AuthSpec::Sso(spec) => {
            let start_url = fields[&spec.start_field].as_str().map(str::trim).unwrap_or_default();
            if start_url.is_empty() {
                return Err(ApiError::bad("Informe a Start URL do IAM Identity Center."));
            }
            let spec = spec.in_region(fields["region"].as_str().unwrap_or_default());
            let client = connectors::oauth::register_client(http, &spec).await?;
            let app = spec.device_for(&client, start_url);
            device(shared, app, Some(client), &opened).await
        }
    }
}

async fn device(
    shared: &Shared,
    app: DeviceApp,
    client: Option<connectors::RegisteredClient>,
    opened: &(dyn Fn(Waiting) -> ApiResult<String> + Sync),
) -> ApiResult<Value> {
    let code = app.start(shared.backend.connectors.http()).await?;
    let url = code.verification_uri_complete.clone().unwrap_or_else(|| code.verification_uri.clone());
    let answer = json!({
        "kind": "device", "url": url, "code": code.user_code, "expiresIn": code.expires_in, "interval": code.interval,
    });
    let flow = opened(Waiting::Device { app, code, client })?;
    let mut answer = answer;
    answer["flow"] = json!(flow);
    Ok(answer)
}

pub async fn callback(shared: &Shared, state: &str, code: &str) -> ApiResult<()> {
    let id = shared.flows.by_state(state).ok_or_else(|| ApiError::missing("Essa entrada expirou."))?;
    let Some(Flow { stage: Stage::Waiting(Waiting::Redirect { exchanger, authorization }), .. }) =
        shared.flows.get(&id)
    else {
        return Err(ApiError::missing("Essa entrada expirou."));
    };
    let http = shared.backend.connectors.http();
    let now = jiff::Timestamp::now();
    let exchanged = match exchanger {
        Exchanger::App(app) => app.exchange(http, &authorization, state, code, now).await,
        Exchanger::Loopback(sign_in) => sign_in.finish(http, &authorization, state, code, now).await,
    };
    match exchanged {
        Ok(grant) => shared.flows.set(&id, Stage::Granted(grant)),
        Err(error) => shared.flows.set(&id, Stage::Failed(error.message)),
    }
    Ok(())
}

pub async fn poll(shared: &Shared, flow_id: &str, choice: Option<&str>) -> ApiResult<Value> {
    let Some(flow) = shared.flows.get(flow_id) else {
        return Ok(json!({ "state": "failed", "message": "A entrada expirou. Comece de novo." }));
    };
    let stage = match flow.stage.clone() {
        Stage::Waiting(Waiting::Device { app, code, client }) => match device_poll(shared, &app, &code, client).await {
            Stage::Granted(grant) => accounts(shared, &flow, grant).await,
            other => other,
        },
        Stage::Waiting(Waiting::Ai(ai)) => crate::ai::poll(shared, &ai),
        Stage::Granted(grant) => accounts(shared, &flow, grant).await,
        Stage::Choose { grant, options } => {
            match choice.and_then(|wanted| options.iter().find(|option| option.id == wanted)) {
                Some(chosen) => finish(shared, &flow.provider, &grant, chosen).await,
                None => Stage::Choose { grant, options },
            }
        }
        other => other,
    };
    shared.flows.set(flow_id, stage.clone());
    Ok(shown(shared, &flow.provider, stage))
}

fn shown(shared: &Shared, provider: &str, stage: Stage) -> Value {
    match stage {
        Stage::Waiting(_) | Stage::Granted(_) => json!({ "state": "pending" }),
        Stage::Choose { options, .. } => {
            let choices: Vec<Value> = options
                .iter()
                .map(|option| json!({ "id": option.id, "label": option.label, "detail": detail(&option.settings) }))
                .collect();
            let key = ProviderKey::from(provider);
            let title = shared.backend.connectors.presentation(&key).map(|look| look.choose_title.clone());
            let title = title.ok().filter(|title| !title.is_empty()).unwrap_or_else(|| "Conta".to_owned());
            json!({ "state": "choose", "title": title, "choices": choices })
        }
        Stage::Done(connection) => json!({ "state": "done", "connection": connection }),
        Stage::Failed(message) => json!({ "state": "failed", "message": message }),
    }
}

fn detail(settings: &Value) -> String {
    if let Some(told) = settings["detail"].as_str() {
        return told.to_owned();
    }
    let texts = settings.as_object().map(|fields| fields.values().filter_map(Value::as_str).collect::<Vec<_>>());
    texts.unwrap_or_default().join(" · ")
}

async fn device_poll(
    shared: &Shared,
    app: &DeviceApp,
    code: &DeviceCode,
    client: Option<connectors::RegisteredClient>,
) -> Stage {
    let http = shared.backend.connectors.http();
    match app.poll(http, &code.device_code, jiff::Timestamp::now()).await {
        Ok(Polled::Pending | Polled::SlowDown { .. }) => {
            Stage::Waiting(Waiting::Device { app: app.clone(), code: code.clone(), client })
        }
        Ok(Polled::Expired) => Stage::Failed("O código expirou. Comece de novo.".to_owned()),
        Ok(Polled::Denied) => Stage::Failed("A entrada foi recusada.".to_owned()),
        Ok(Polled::Authorized(mut grant)) => {
            grant.client = client;
            Stage::Granted(grant)
        }
        Err(error) => Stage::Failed(error.message),
    }
}

fn connection(id: &str, descriptor: &ProviderDescriptor) -> Connection {
    Connection {
        id: ConnectionId::from(id),
        provider: descriptor.key.clone(),
        label: descriptor.name.clone(),
        account: String::new(),
        capabilities: descriptor.capabilities.clone(),
    }
}

async fn accounts(shared: &Shared, flow: &Flow, mut grant: Grant) -> Stage {
    let provider = flow.provider.as_str();
    let Ok(found) = descriptor(shared, provider) else { return Stage::Failed("Provedor desconhecido.".to_owned()) };
    grant.method.clone_from(&flow.method);
    let region = flow.fields["region"].as_str().filter(|region| !region.trim().is_empty());
    if let (Some(region), true) = (region, grant.extra.is_null()) {
        grant.extra = json!({ "region": region.trim() });
    }
    let probe = format!("probe-{}", random_id().unwrap_or_default());
    let keeper = shared.backend.connectors.keeper();
    if let Err(error) = keeper.keep(&ConnectionId::from(probe.as_str()), &grant).await {
        return Stage::Failed(error.message);
    }
    let settings = region.map_or_else(|| json!({}), |region| json!({ "region": region.trim() }));
    let listed = shared.backend.connectors.accounts(&connection(&probe, &found), &settings).await;
    let _ = keeper.store().remove(&ConnectionId::from(probe.as_str())).await;
    let chooses = shared.backend.connectors.chooses(&found.key);
    match listed {
        Ok(options) if options.len() > 1 => Stage::Choose { grant, options },
        Ok(options) if options.is_empty() && !chooses.is_empty() => Stage::Failed(nothing_to_choose(&found)),
        Ok(options) => {
            let fallback = AccountOption { id: String::new(), label: found.name.clone(), settings: json!({}) };
            finish(shared, provider, &grant, options.first().unwrap_or(&fallback)).await
        }
        Err(error) => Stage::Failed(error.message),
    }
}

fn nothing_to_choose(found: &ProviderDescriptor) -> String {
    format!("A conta do {} não tem nada para escolher. Confira o acesso da conta e conecte de novo.", found.name)
}

async fn finish(shared: &Shared, provider: &str, grant: &Grant, chosen: &AccountOption) -> Stage {
    let Ok(found) = descriptor(shared, provider) else { return Stage::Failed("Provedor desconhecido.".to_owned()) };
    let records = shared.backend.store.connections().unwrap_or_default();
    let same =
        records.iter().find(|record| record.provider == provider && record.account.as_deref() == Some(&chosen.label));
    let id = same.map_or_else(|| format!("c-{}", random_id().unwrap_or_default()), |record| record.id.clone());
    let record = ConnectionRecord {
        id: id.clone(),
        provider: provider.to_owned(),
        label: chosen.label.clone(),
        account: Some(chosen.label.clone()),
        capabilities: found.capabilities.iter().map(|capability: &Capability| capability.0.clone()).collect(),
        settings: chosen.settings.clone(),
    };
    if let Err(error) = shared.backend.connectors.keeper().keep(&ConnectionId::from(id.as_str()), grant).await {
        return Stage::Failed(error.message);
    }
    if let Err(error) = shared.backend.store.save_connection(&record) {
        return Stage::Failed(error.to_string());
    }
    if let Some(health) = shared.backend.orchestrator.health() {
        health.reconnected(&ConnectionId::from(id.as_str()));
    }
    crate::connections::announce(shared);
    Stage::Done(crate::connections::record_json(&record, "ok"))
}
