use domain::Language;
use domain::connection::ProviderKey;
use serde::Serialize;

use super::{Connectors, Entry};
use crate::catalog::EventSpec;
use crate::provider::ExternalEvent;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Catalogued {
    pub connector: ProviderKey,
    pub name: String,
    pub events: Vec<EventSpec>,
}

impl Entry {
    pub(crate) fn named(&self, event: &mut ExternalEvent) {
        if self.events.is_empty() {
            return;
        }
        let Some(spec) = self.events.iter().find(|spec| spec.answers(&event.kind)) else {
            tracing::warn!(
                connector = %self.descriptor.key,
                kind = %event.kind,
                "the connector polled an event its events() does not declare"
            );
            return;
        };
        event.kind.clone_from(&spec.name);
        event.aliases.clone_from(&spec.aliases);
    }
}

impl Connectors {
    pub fn catalog(&self, language: Language) -> Vec<Catalogued> {
        self.entries
            .iter()
            .filter(|entry| !entry.events.is_empty())
            .map(|entry| Catalogued {
                connector: entry.descriptor.key.clone(),
                name: entry.descriptor.name.clone(),
                events: entry.provider.catalog(language),
            })
            .collect()
    }

    pub fn declared(&self) -> impl Iterator<Item = &EventSpec> {
        self.entries.iter().flat_map(|entry| entry.events.iter())
    }

    pub fn event(&self, kind: &str) -> Option<&EventSpec> {
        self.declared().find(|spec| spec.name == kind).or_else(|| self.declared().find(|spec| spec.answers(kind)))
    }

    pub fn canonical(&self, provider: &ProviderKey, event: &mut ExternalEvent) {
        if let Ok(entry) = self.entry(provider) {
            entry.named(event);
        }
    }
}
