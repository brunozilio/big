pub mod aws;
pub mod github;
pub mod jira;
pub mod zephyr;

use crate::provider::Provider;

pub static ALL: &[&dyn Provider] = &[&github::Github, &jira::Jira, &aws::Aws, &zephyr::Zephyr];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_provider_has_a_unique_key_a_logo_and_only_actions_of_its_capabilities() {
        let mut keys = BTreeSet::new();
        for provider in ALL {
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
