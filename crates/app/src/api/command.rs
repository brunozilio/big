use std::path::PathBuf;

use domain::provider::ModelProvider;
use domain::{AskId, Decision, PromptRequest, RunId, SessionId};
use serde::{Deserialize, Serialize};

use crate::api::RequestId;

pub const SIGN_IN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant, reason = "commands are short-lived messages, never stored in bulk")]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ClientCommand {
    SubmitPrompt {
        run: RunId,
        request: PromptRequest,
    },
    CancelRun {
        run: RunId,
    },
    AnswerAsk {
        run: RunId,
        ask: AskId,
        decision: Decision,
    },
    SteerRun {
        run: RunId,
        text: String,
    },
    ListModels,
    ListProviders,
    SignInProvider {
        provider: ModelProvider,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        back: Option<String>,
        #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
        fields: serde_json::Value,
    },
    DisconnectProvider {
        provider: ModelProvider,
    },
    RemoveProviderAccount {
        provider: ModelProvider,
        account: String,
    },
    ListSessions,
    SessionHistory {
        session: SessionId,
        cwd: PathBuf,
    },
    CancelRequest {
        request: RequestId,
    },
    BackgroundJobs,
    StopBackground {
        id: String,
    },
}

impl ClientCommand {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SubmitPrompt { .. } => "submit_prompt",
            Self::CancelRun { .. } => "cancel_run",
            Self::AnswerAsk { .. } => "answer_ask",
            Self::SteerRun { .. } => "steer_run",
            Self::ListModels => "list_models",
            Self::ListProviders => "list_providers",
            Self::SignInProvider { .. } => "sign_in_provider",
            Self::DisconnectProvider { .. } => "disconnect_provider",
            Self::RemoveProviderAccount { .. } => "remove_provider_account",
            Self::ListSessions => "list_sessions",
            Self::SessionHistory { .. } => "session_history",
            Self::CancelRequest { .. } => "cancel_request",
            Self::BackgroundJobs => "background_jobs",
            Self::StopBackground { .. } => "stop_background",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ModelOption;

    #[test]
    fn every_command_round_trips() {
        let run = RunId::new();
        let request = PromptRequest::plain("/repo".into(), "hi", ModelOption::new("m", "M"));
        let commands = [
            ClientCommand::SubmitPrompt { run, request },
            ClientCommand::CancelRun { run },
            ClientCommand::AnswerAsk { run, ask: AskId("a".into()), decision: Decision::Answer(vec![vec![0]]) },
            ClientCommand::SteerRun { run, text: "also the tests".into() },
            ClientCommand::ListModels,
            ClientCommand::ListProviders,
            ClientCommand::SignInProvider { provider: ModelProvider::new("browser-sub"), back: None, fields: serde_json::Value::Null },
            ClientCommand::SignInProvider {
                provider: ModelProvider::new("browser-sub"),
                back: Some("http://127.0.0.1:7999/settings".into()),
                fields: serde_json::json!({ "model": "owner/repo" }),
            },
            ClientCommand::DisconnectProvider { provider: ModelProvider::new("keyed") },
            ClientCommand::RemoveProviderAccount { provider: ModelProvider::new("keyed"), account: "a@x.io".into() },
            ClientCommand::ListSessions,
            ClientCommand::SessionHistory { session: SessionId("s".into()), cwd: "/repo".into() },
            ClientCommand::CancelRequest { request: RequestId(7) },
            ClientCommand::BackgroundJobs,
            ClientCommand::StopBackground { id: "bash_1".into() },
        ];
        for command in commands {
            let json = serde_json::to_string(&command).expect("serialize");
            assert_eq!(serde_json::from_str::<ClientCommand>(&json).expect("deserialize"), command);
        }
    }
}
