use crate::api::{ClientCommand, ProviderEvent, Reply};
use domain::ModelOption;
use domain::provider::{ModelProvider, ProviderSummary};

use crate::{Client, Error};

impl Client {
    pub fn models(&self) -> Result<Vec<ModelOption>, Error> {
        self.dispatcher.call(ClientCommand::ListModels, |reply| match reply {
            Reply::Models(models) => Some(Ok(models)),
            _ => None,
        })
    }

    pub fn providers(&self) -> Result<Vec<ProviderSummary>, Error> {
        self.dispatcher.call(ClientCommand::ListProviders, listed)
    }

    pub fn sign_in_returning(
        &self,
        provider: ModelProvider,
        back: &str,
        fields: serde_json::Value,
        mut open: impl FnMut(&str, Option<&str>),
    ) -> Result<SignedIn, Error> {
        let command = ClientCommand::SignInProvider { provider, back: Some(back.to_owned()), fields };
        self.dispatcher.flow(command, crate::client::dispatch::sign_in_patience(), |reply| match reply {
            Reply::Providers(ProviderEvent::AuthorizeAt { url, code, .. }) => {
                open(&url, code.as_deref());
                None
            }
            Reply::Providers(ProviderEvent::PortBusy { port, holder, .. }) => {
                Some(Ok(SignedIn::PortBusy { port, holder }))
            }
            other => listed(other).map(|answer| answer.map(SignedIn::Connected)),
        })
    }

    pub fn disconnect_provider(&self, provider: ModelProvider) -> Result<Vec<ProviderSummary>, Error> {
        self.dispatcher.call(ClientCommand::DisconnectProvider { provider }, listed)
    }

    pub fn remove_account(&self, provider: ModelProvider, account: &str) -> Result<Vec<ProviderSummary>, Error> {
        self.dispatcher.call(ClientCommand::RemoveProviderAccount { provider, account: account.to_owned() }, listed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignedIn {
    Connected(Vec<ProviderSummary>),
    PortBusy { port: u16, holder: Option<domain::provider::PortHolder> },
}

fn listed(reply: Reply) -> Option<Result<Vec<ProviderSummary>, String>> {
    match reply {
        Reply::Providers(ProviderEvent::Listed { providers }) => Some(Ok(providers)),
        Reply::Providers(ProviderEvent::Failed { message, .. }) => Some(Err(message)),
        _ => None,
    }
}
