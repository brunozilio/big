use std::sync::Arc;

use crate::provider::Provider;

pub fn all() -> Vec<Arc<dyn Provider>> {
    crate::scripted::discover()
}

#[derive(Debug)]
pub struct Shared(pub &'static dyn Provider);

#[async_trait::async_trait]
impl Provider for Shared {
    fn descriptor(&self) -> crate::descriptor::ProviderDescriptor {
        self.0.descriptor()
    }

    async fn call(
        &self,
        http: &dyn crate::http::Http,
        creds: &crate::provider::Credentials,
        action: &str,
        args: &serde_json::Value,
        idempotency_key: &str,
    ) -> Result<serde_json::Value, crate::error::ActionError> {
        self.0.call(http, creds, action, args, idempotency_key).await
    }

    async fn accounts(
        &self,
        http: &dyn crate::http::Http,
        creds: &crate::provider::Credentials,
    ) -> Result<Vec<crate::provider::AccountOption>, crate::error::ActionError> {
        self.0.accounts(http, creds).await
    }

    fn events(&self) -> Option<&dyn crate::provider::EventSource> {
        self.0.events()
    }

    fn presentation(&self, language: domain::Language) -> crate::descriptor::Presentation {
        self.0.presentation(language)
    }

    fn alternatives(&self, language: domain::Language) -> Vec<crate::descriptor::AlternateAuth> {
        self.0.alternatives(language)
    }

    fn chooses(&self) -> Vec<String> {
        self.0.chooses()
    }

    fn requires(&self) -> domain::data_policy::Requires {
        self.0.requires()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_provider_has_a_unique_key_a_logo_and_only_actions_of_its_capabilities() {
        let mut keys = BTreeSet::new();
        for provider in all() {
            let descriptor = provider.descriptor();
            assert!(keys.insert(descriptor.key.clone()), "{} twice", descriptor.key);
            assert!(descriptor.logo.starts_with("<svg"), "{}", descriptor.key);
            assert!(!descriptor.actions.is_empty());
            for action in &descriptor.actions {
                let capability = action.capability();
                assert!(
                    capability.is_some_and(|c| descriptor.capabilities.contains(&c)),
                    "{} offers {} outside its capabilities",
                    descriptor.key,
                    action.name
                );
            }
        }
    }
}
