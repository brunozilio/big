use std::sync::Arc;

use domain::billing::{PlanLimits, PlanUsage, PlanWindow};
use domain::provider::{ModelProvider, ProviderKey};

use super::gather;
use crate::providers::{InMemoryProviderStore, ModelProviders};
use crate::secrets::InMemorySecretStore;

const KEYED: ModelProvider = ModelProvider::new("keyed");

fn rig() -> ModelProviders {
    llm_providers::testing::install();
    ModelProviders::new(Arc::new(InMemoryProviderStore::default()), Arc::new(InMemorySecretStore::default()))
        .with_clock(|| 1_000)
}

#[test]
fn every_account_is_listed_without_turning_the_rotation() {
    let providers = rig();
    assert!(providers.accounts_of(KEYED).is_empty());
    providers.connect(KEYED, "sk-proj-0123456789aaaa");
    providers.connect(KEYED, "sk-proj-0123456789bbbb");
    let labels = |providers: &ModelProviders| {
        providers.accounts_of(KEYED).into_iter().map(|(label, _)| label).collect::<Vec<_>>()
    };
    assert_eq!(labels(&providers), ["…aaaa", "…bbbb"]);
    assert_eq!(labels(&providers), ["…aaaa", "…bbbb"]);
}

#[test]
fn each_account_answers_on_its_own_and_one_failing_spoils_none_of_the_others() {
    let usage = PlanUsage {
        plan: Some("Max".into()),
        windows: vec![PlanWindow {
            id: "w".into(),
            label: "W".into(),
            used_percent: 5.0,
            resets_at: None,
            starts_at: None,
        }],
    };
    let accounts = ["ok", "none", "bad"].map(|label| (KEYED, label.to_owned(), ProviderKey::new(label)));
    let answer = usage.clone();
    let listed = gather(accounts.to_vec(), &move |_, key: ProviderKey| match key
        .current()
        .map(|key| key.secret().to_owned())
        .as_deref()
    {
        Ok("ok") => Ok(Some(answer.clone())),
        Ok("none") => Ok(None),
        _ => Err("caiu".to_owned()),
    });
    let shown: Vec<_> = listed.into_iter().map(|account| (account.account, account.limits)).collect();
    assert_eq!(
        shown,
        [
            ("ok".to_owned(), PlanLimits::Known(usage)),
            ("none".to_owned(), PlanLimits::Unsupported),
            ("bad".to_owned(), PlanLimits::Failed("caiu".to_owned())),
        ]
    );
}
