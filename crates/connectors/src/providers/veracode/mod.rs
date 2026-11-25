mod findings;
mod sign;

use async_trait::async_trait;
use domain::Language;
use domain::connection::{Capability, ProviderKey};
use serde_json::{Value, json};

use crate::clock::{Clock, SystemClock};
use crate::contracts::{self, security as actions};
use crate::descriptor::{AuthSpec, Presentation, ProviderDescriptor, TokenField, TokenForm, TokenSpec};
use crate::error::ActionError;
use crate::http::{Http, HttpRequest, fetch_json};
use crate::provider::{AccountOption, Args, Credentials, Provider, text_of, unknown_action};

use findings::Filter;
pub use sign::Keys;

pub const KEY: &str = "veracode";
pub const REGION: &str = "region";
pub use crate::texts::NO_PROJECT;
pub const REGIONS: [(&str, &str, &str); 3] =
    [("com", "api.veracode.com", ""), ("eu", "api.veracode.eu", " EU"), ("us", "api.veracode.us", " FedRAMP")];
const LOGO: &str = include_str!("logo.svg");
const APPLICATIONS: &str = "/appsec/v1/applications";
const APPLICATION_PAGE: &str = "100";
const FINDING_PAGE: &str = "500";
const MOST_PAGES: u64 = 10;

#[derive(Debug, Clone, Copy, Default)]
pub struct Veracode;

pub type Nonce = fn() -> Result<Vec<u8>, ActionError>;

pub fn random_nonce() -> Result<Vec<u8>, ActionError> {
    platform::crypto::random(16).map_err(ActionError::permanent)
}

pub fn region_of(entered: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let bare = entered.trim().trim_start_matches("https://").trim_end_matches('/').to_ascii_lowercase();
    let bare = bare.strip_prefix("api.veracode.").unwrap_or(&bare);
    let bare = if bare.is_empty() { brand::connectors::VERACODE_REGION } else { bare };
    REGIONS.iter().copied().find(|(region, _, _)| *region == bare)
}

fn unknown_region(entered: &str) -> ActionError {
    let known: Vec<&str> = REGIONS.iter().map(|(region, _, _)| *region).collect();
    ActionError::permanent(format!("{entered:?} is not a Veracode region; use {}", known.join(", ")))
}

fn region_field(language: Language) -> TokenField {
    let label = language.pick("Veracode region", "Región de Veracode", "Região do Veracode");
    TokenField::setting(REGION, label, "").filled(brand::connectors::VERACODE_REGION)
}

pub fn token_spec(language: Language) -> TokenSpec {
    let id = TokenField::new("apiId", "API ID", "", false);
    let key = TokenField::new("apiKey", "API key", "", true);
    TokenSpec { fields: vec![region_field(language), id, key], form: TokenForm::Json }
}

#[derive(Debug)]
pub struct Api<'a> {
    http: &'a dyn Http,
    keys: Keys,
    host: &'static str,
    label: String,
    region: &'static str,
    project: Option<String>,
    clock: &'a dyn Clock,
    nonce: Nonce,
}

impl<'a> Api<'a> {
    pub fn new(http: &'a dyn Http, creds: &Credentials, clock: &'a dyn Clock, nonce: Nonce) -> Result<Api<'a>, ActionError> {
        let keys = Keys::read(&creds.access_token).ok_or_else(ActionError::reconnect)?;
        let entered = creds.setting(REGION).unwrap_or_default();
        let (region, host, suffix) = region_of(entered).ok_or_else(|| unknown_region(entered))?;
        Ok(Api {
            http,
            keys,
            host,
            label: format!("Veracode{suffix}"),
            region,
            project: creds.setting("project").map(str::to_owned),
            clock,
            nonce,
        })
    }

    pub async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, ActionError> {
        let mut url = url::Url::parse(&format!("https://{}{path}", self.host))
            .map_err(|error| ActionError::permanent(error.to_string()))?;
        url.query_pairs_mut().extend_pairs(query);
        let path_and_query = &url[url::Position::BeforePath..url::Position::AfterQuery];
        let request = sign::Request { method: "GET", host: self.host, path_and_query };
        let nonce = (self.nonce)()?;
        let millis = self.clock.now().as_millisecond();
        let authorization = sign::header(&self.keys, request, millis, &nonce)
            .ok_or_else(|| ActionError::auth("the Veracode API key is not valid hex; generate a new one"))?;
        let request = HttpRequest::get(String::from(url.clone())).header("authorization", authorization);
        fetch_json(self.http, request).await
    }
}

fn project_of(application: &Value) -> Option<Value> {
    let guid = text_of(application, "/guid");
    let name = text_of(application, "/profile/name");
    (!guid.is_empty()).then(|| json!({ "id": guid, "key": guid, "name": if name.is_empty() { guid.clone() } else { name } }))
}

fn last_page(answer: &Value, page: u64, fetched: usize) -> bool {
    let total = answer["page"]["total_pages"].as_u64().unwrap_or(0);
    fetched == 0 || page + 1 >= total
}

pub async fn applications(api: &Api<'_>, name: Option<&str>) -> Result<Vec<Value>, ActionError> {
    let mut found = Vec::new();
    for page in 0..MOST_PAGES {
        let number = page.to_string();
        let mut query = vec![("page", number.as_str()), ("size", APPLICATION_PAGE)];
        if let Some(name) = name {
            query.push(("name", name));
        }
        let answer = api.get(APPLICATIONS, &query).await?;
        let listed = answer["_embedded"]["applications"].as_array().map(Vec::as_slice).unwrap_or_default();
        found.extend(listed.iter().filter_map(project_of));
        if last_page(&answer, page, listed.len()) {
            break;
        }
    }
    Ok(found)
}

async fn projects(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let query = args.optional_text("query").map(str::trim).filter(|query| !query.is_empty());
    let lowered = query.map(str::to_lowercase).unwrap_or_default();
    let matches = |project: &Value| {
        lowered.is_empty()
            || text_of(project, "/name").to_lowercase().contains(&lowered)
            || text_of(project, "/key").to_lowercase().contains(&lowered)
    };
    Ok(Value::Array(applications(api, query).await?.into_iter().filter(matches).collect()))
}

pub fn is_guid(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let sizes: Vec<usize> = parts.iter().map(|part| part.len()).collect();
    sizes == [8, 4, 4, 4, 12] && parts.iter().all(|part| part.chars().all(|c| c.is_ascii_hexdigit()))
}

async fn project_guid(api: &Api<'_>, args: Args<'_>) -> Result<String, ActionError> {
    let given = args.optional_text("project").map(str::trim).filter(|project| !project.is_empty());
    let given = given.or(api.project.as_deref()).ok_or_else(|| ActionError::blocked(NO_PROJECT))?;
    if is_guid(given) {
        return Ok(given.to_owned());
    }
    let found = applications(api, Some(given)).await?;
    let exact: Vec<&Value> =
        found.iter().filter(|project| text_of(project, "/name").eq_ignore_ascii_case(given)).collect();
    let chosen = match (exact.as_slice(), found.as_slice()) {
        ([only], _) => Ok(*only),
        ([], [only]) => Ok(only),
        ([], []) => Err(ActionError::permanent(format!("no Veracode application is named {given:?}"))),
        _ => Err(ActionError::permanent(format!("several Veracode applications match {given:?}; give its GUID"))),
    };
    Ok(text_of(chosen?, "/id"))
}

async fn scan_findings(api: &Api<'_>, path: &str, scan: &str, filter: &Filter) -> Result<Vec<Value>, ActionError> {
    let mut found = Vec::new();
    let severity = filter.min_severity.to_string();
    for page in 0..MOST_PAGES {
        let number = page.to_string();
        let mut query = vec![("size", FINDING_PAGE), ("page", number.as_str()), ("scan_type", scan)];
        if filter.policy {
            query.push(("violates_policy", "TRUE"));
        }
        if filter.min_severity > 0 {
            query.push(("severity_gte", severity.as_str()));
        }
        let answer = api.get(path, &query).await?;
        let listed = answer["_embedded"]["findings"].as_array().map(Vec::as_slice).unwrap_or_default();
        found.extend(listed.iter().cloned());
        if last_page(&answer, page, listed.len()) {
            break;
        }
    }
    Ok(found)
}

async fn security_findings(api: &Api<'_>, args: Args<'_>) -> Result<Value, ActionError> {
    let filter = Filter::of(args)?;
    let guid = project_guid(api, args).await?;
    let path = format!("/appsec/v2/applications/{guid}/findings");
    let mut raw = Vec::new();
    for scan in &filter.scan_types {
        raw.extend(scan_findings(api, &path, scan, &filter).await?);
    }
    Ok(findings::chosen(&raw, &filter))
}

pub async fn run(api: &Api<'_>, action: &str, args: &Value) -> Result<Value, ActionError> {
    let args = Args(args);
    match action {
        actions::PROJECTS => projects(api, args).await,
        actions::FINDINGS => security_findings(api, args).await,
        _ => Err(unknown_action(action)),
    }
}

pub async fn options(api: &Api<'_>) -> Result<Vec<AccountOption>, ActionError> {
    let found = applications(api, None).await?;
    let names: Vec<String> = found.iter().map(|project| text_of(project, "/name")).collect();
    let mut settings = json!({ REGION: api.region, "shown": { "projects": names } });
    if let [only] = found.as_slice() {
        settings["project"] = json!(text_of(only, "/key"));
    }
    Ok(vec![AccountOption { id: format!("{KEY}-{}", api.region), label: api.label.clone(), settings }])
}

#[async_trait]
impl Provider for Veracode {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            key: ProviderKey::from(KEY),
            name: "Veracode".to_owned(),
            logo: LOGO.to_owned(),
            capabilities: vec![Capability::from(contracts::SECURITY)],
            auth: AuthSpec::Token(token_spec(Language::default())),
            actions: contracts::security(),
        }
    }

    fn presentation(&self, language: Language) -> Presentation {
        Presentation {
            description: language.pick("Your Veracode account", "Tu cuenta de Veracode", "Sua conta Veracode").to_owned(),
            group: contracts::SECURITY.to_owned(),
            button: crate::texts::connect(language).to_owned(),
            site: language.pick("Veracode", "de Veracode", "do Veracode").to_owned(),
            choose_title: language.pick("Veracode account", "Cuenta de Veracode", "Conta do Veracode").to_owned(),
            hidden: false,
        }
    }

    async fn call(
        &self,
        http: &dyn Http,
        creds: &Credentials,
        action: &str,
        args: &Value,
        _idempotency_key: &str,
    ) -> Result<Value, ActionError> {
        run(&Api::new(http, creds, &SystemClock, random_nonce)?, action, args).await
    }

    async fn accounts(&self, http: &dyn Http, creds: &Credentials) -> Result<Vec<AccountOption>, ActionError> {
        options(&Api::new(http, creds, &SystemClock, random_nonce)?).await
    }
}

#[cfg(test)]
mod tests;
