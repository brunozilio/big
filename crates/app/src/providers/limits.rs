use std::sync::Arc;

use agent_engine::model::client::ProviderClient;
use agent_engine::model::http::NetHttp;
use domain::billing::{AccountLimits, PlanLimits, PlanUsage};
use domain::provider::{ModelProvider, ProviderKey};

use super::ModelProviders;

pub type Ask = dyn Fn(ModelProvider, ProviderKey) -> Result<Option<PlanUsage>, String> + Sync;

impl ModelProviders {
    pub fn accounts_of(&self, provider: ModelProvider) -> Vec<(String, ProviderKey)> {
        let book = self.store.load();
        let kept: Vec<_> = book
            .accounts(provider)
            .iter()
            .filter_map(|account| Some((account.label.clone(), self.account_key(provider, &account.id).ok()?)))
            .collect();
        if !kept.is_empty() {
            return kept;
        }
        self.key(provider).map(|key| vec![(agent_engine::providers::name(provider), key)]).unwrap_or_default()
    }

    pub fn plan_limits(&self) -> Vec<AccountLimits> {
        let accounts = agent_engine::providers::all()
            .into_iter()
            .filter(|known| known.access().subscription())
            .map(llm_providers::id)
            .flat_map(|provider| self.accounts_of(provider).into_iter().map(move |(label, key)| (provider, label, key)))
            .collect();
        gather(accounts, &ask)
    }
}

fn ask(provider: ModelProvider, key: ProviderKey) -> Result<Option<PlanUsage>, String> {
    ProviderClient::new(provider, Arc::new(NetHttp), Arc::new(key)).limits().map_err(|error| error.to_string())
}

pub fn gather(accounts: Vec<(ModelProvider, String, ProviderKey)>, ask: &Ask) -> Vec<AccountLimits> {
    std::thread::scope(|scope| {
        let asks: Vec<_> = accounts
            .into_iter()
            .map(|(provider, account, key)| (provider, account, scope.spawn(move || ask(provider, key))))
            .collect();
        asks.into_iter()
            .map(|(provider, account, asked)| {
                let limits = match asked.join() {
                    Ok(Ok(Some(usage))) => PlanLimits::Known(usage),
                    Ok(Ok(None)) => PlanLimits::Unsupported,
                    Ok(Err(message)) => PlanLimits::Failed(message),
                    Err(_) => PlanLimits::Failed("a consulta dos limites falhou".to_owned()),
                };
                AccountLimits { provider, account, limits }
            })
            .collect()
    })
}

#[cfg(test)]
#[path = "limits_tests.rs"]
mod tests;
