use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use connectors::contracts;
use domain::effect::{Call, ErrorClass};
use serde_json::{Value, json};
use store::Binding;

use crate::agent::Failure;
use crate::effects::connector;
use crate::protocol::Command;
use crate::shared::Shared;

pub const CREDENTIALS_COMMAND: &str = "cloud-credentials";
pub const CREDENTIALS_PATH: &str = "/api/cloud/credentials/";

#[derive(Debug, Default)]
pub struct Leases(Mutex<HashMap<String, Call>>);

impl Leases {
    fn held(&self) -> std::sync::MutexGuard<'_, HashMap<String, Call>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn call(&self, token: &str) -> Option<Call> {
        self.held().get(token).cloned()
    }
}

#[derive(Debug)]
pub struct Lease {
    shared: Arc<Shared>,
    token: String,
    folder: PathBuf,
    pub environment: Vec<(String, String)>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.shared.clouds.held().remove(&self.token);
        if let Err(error) = std::fs::remove_dir_all(&self.folder) {
            tracing::debug!(%error, "the cloud profile folder could not be removed");
        }
    }
}

fn wanted(command: &Command, chosen: Option<&Binding>) -> Option<(Call, bool)> {
    let asked = match &command.args["cloud"] {
        Value::Bool(true) => None,
        Value::String(id) if !id.is_empty() => Some(id.clone()),
        _ => return None,
    };
    let optional = asked.is_none();
    let chosen = chosen.filter(|chosen| asked.as_ref().is_none_or(|id| id == &chosen.connection));
    let mut args = json!({});
    if let Some(chosen) = chosen {
        args["binding"] = json!(chosen.value);
    }
    let connection = asked.or_else(|| chosen.map(|chosen| chosen.connection.clone()));
    let call = Call {
        capability: contracts::CLOUD.to_owned(),
        action: contracts::cloud::IDENTITY.to_owned(),
        connection,
        args,
    };
    Some((call, optional))
}

fn chosen(shared: &Shared, input: &Value) -> Option<Binding> {
    let id = crate::application::id_of(input)?;
    let application = shared.store.application(&id).ok().flatten()?;
    application.bound(contracts::CLOUD).cloned()
}

fn token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn profile_text(url: &str, region: Option<&str>) -> Result<String, Failure> {
    let program = std::env::current_exe().map_err(|error| Failure::permanent(error.to_string()))?;
    let mut text = format!(
        "[profile {}]\ncredential_process = {} {CREDENTIALS_COMMAND} {url}\n",
        brand::COMMAND,
        quoted(&program.to_string_lossy())
    );
    if let Some(region) = region {
        text.push_str(&format!("region = {region}\n"));
    }
    Ok(text)
}

fn write_profile(folder: &std::path::Path, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    platform::make_private_dir(folder)?;
    let file = folder.join("config");
    platform::private_file::write(&file, text.as_bytes())?;
    Ok(file)
}

fn environment(file: &std::path::Path, region: Option<&str>) -> Vec<(String, String)> {
    let mut environment = vec![
        ("AWS_CONFIG_FILE".to_owned(), file.to_string_lossy().into_owned()),
        ("AWS_PROFILE".to_owned(), brand::COMMAND.to_owned()),
        ("AWS_SDK_LOAD_CONFIG".to_owned(), "1".to_owned()),
    ];
    if let Some(region) = region {
        environment.push(("AWS_REGION".to_owned(), region.to_owned()));
        environment.push(("AWS_DEFAULT_REGION".to_owned(), region.to_owned()));
    }
    environment
}

pub async fn lease(shared: &Arc<Shared>, command: &Command, input: &Value) -> Result<Option<Lease>, Failure> {
    let Some((call, optional)) = wanted(command, chosen(shared, input).as_ref()) else { return Ok(None) };
    let (connection, _) = match crate::effects::connection_of(shared, &call) {
        Ok(found) => found,
        Err(Failure::Blocked(_)) if optional => return Ok(None),
        Err(failure) => return Err(failure),
    };
    let base = shared.config.server.clone().ok_or_else(|| Failure::permanent("the server address is not known"))?;
    let call = Call { connection: Some(connection.id.to_string()), ..call };
    let token = token();
    let identity = connector(shared, &call, &format!("{}:{CREDENTIALS_COMMAND}:{token}", brand::COMMAND)).await?;
    let region = identity["region"].as_str().filter(|region| !region.is_empty()).map(str::to_owned);
    let url = format!("{}{CREDENTIALS_PATH}{token}", base.trim_end_matches('/'));
    let text = profile_text(&url, region.as_deref())?;
    let folder = shared.config.data.join("cloud").join(&token);
    shared.clouds.held().insert(token.clone(), call);
    let mut lease = Lease { shared: Arc::clone(shared), token, folder: folder.clone(), environment: Vec::new() };
    let file = write_profile(&folder, &text).map_err(|error| {
        Failure::Class(ErrorClass::Transient, format!("the cloud profile could not be kept: {error}"))
    })?;
    lease.environment = environment(&file, region.as_deref());
    Ok(Some(lease))
}

pub fn process_credentials(identity: &Value) -> Value {
    let mut answer = json!({
        "Version": 1,
        "AccessKeyId": identity["access_key_id"],
        "SecretAccessKey": identity["secret_access_key"],
    });
    if identity["session_token"].as_str().is_some_and(|token| !token.is_empty()) {
        answer["SessionToken"] = identity["session_token"].clone();
    }
    let expiration = identity["expires_at"].as_str().and_then(|at| at.parse::<jiff::Timestamp>().ok());
    if let Some(at) = expiration {
        answer["Expiration"] = json!(at.to_string());
    }
    answer
}

pub async fn credentials(shared: &Shared, token: &str) -> crate::error::Result<Value> {
    let call = shared.clouds.call(token).ok_or(crate::error::Error::UnknownCloudToken)?;
    let key = format!("{}:{CREDENTIALS_COMMAND}:{}", brand::COMMAND, uuid::Uuid::new_v4().simple());
    match connector(shared, &call, &key).await {
        Ok(identity) => Ok(process_credentials(&identity)),
        Err(Failure::Class(_, message) | Failure::Blocked(message)) => Err(crate::error::Error::Cloud(message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_becomes_what_a_credential_process_prints() {
        let identity = json!({
            "access_key_id": "AK", "secret_access_key": "SK", "session_token": "ST",
            "region": "sa-east-1", "expires_at": "2100-01-01T00:00:00Z",
        });
        assert_eq!(
            process_credentials(&identity),
            json!({ "Version": 1, "AccessKeyId": "AK", "SecretAccessKey": "SK", "SessionToken": "ST",
                    "Expiration": "2100-01-01T00:00:00Z" })
        );
        let unknown = process_credentials(&json!({ "access_key_id": "AK", "session_token": "", "expires_at": "" }));
        assert!(unknown.get("Expiration").is_none());
        assert!(unknown.get("SessionToken").is_none());
    }

    fn command(cloud: Value) -> Command {
        Command {
            seq: 1,
            kind: crate::protocol::AGENT.into(),
            branch: String::new(),
            step: None,
            attempt: 0,
            args: json!({ "cloud": cloud }),
            connection: None,
            retry: None,
            timeout: None,
            optional: false,
        }
    }

    #[test]
    fn an_agent_asks_for_the_cloud_with_true_or_a_connection() {
        let asked = |cloud: Value| wanted(&command(cloud), None).map(|(call, optional)| (call.connection, optional));
        assert_eq!(asked(json!(true)), Some((None, true)));
        assert_eq!(asked(json!("aws-1")), Some((Some("aws-1".to_owned()), false)));
        assert_eq!(wanted(&command(json!(true)), None).map(|(call, _)| call.args), Some(json!({})));
        assert!(wanted(&command(json!(false)), None).is_none());
        assert!(wanted(&command(Value::Null), None).is_none());
    }

    #[test]
    fn the_account_and_role_chosen_for_the_application_go_to_the_identity_call() {
        let chosen = Binding::new("cl-1", "111/Deploy");
        let Some((call, optional)) = wanted(&command(json!(true)), Some(&chosen)) else { panic!("no call") };
        assert_eq!(call.connection.as_deref(), Some("cl-1"));
        assert_eq!(call.args, json!({ "binding": "111/Deploy" }));
        assert_eq!(call.action, contracts::cloud::IDENTITY);
        assert!(optional, "an agent that only wants the cloud still runs without it");
        let same = wanted(&command(json!("cl-1")), Some(&chosen)).map(|(call, _)| call.args);
        assert_eq!(same, Some(json!({ "binding": "111/Deploy" })));
        let other = wanted(&command(json!("cl-2")), Some(&chosen)).map(|(call, _)| (call.connection, call.args));
        assert_eq!(other, Some((Some("cl-2".to_owned()), json!({}))));
    }

    #[test]
    fn the_profile_runs_this_program_with_the_token_url() {
        let text = profile_text("http://127.0.0.1:9/api/cloud/credentials/t", Some("us-east-1")).expect("profile");
        assert!(text.starts_with(&format!("[profile {}]\n", brand::COMMAND)));
        assert!(text.contains(&format!("{CREDENTIALS_COMMAND} http://127.0.0.1:9/api/cloud/credentials/t\n")));
        assert!(text.ends_with("region = us-east-1\n"));
    }
}
