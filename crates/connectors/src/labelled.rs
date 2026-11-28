use domain::connection::Connection;
use domain::label::{Read, Trust};
use domain::origin::Attribution;
use serde_json::Value;

use crate::error::ActionError;
use crate::hub::Connectors;

#[derive(Debug, Clone, PartialEq)]
pub struct Labelled {
    pub value: Value,
    pub read: Read,
}

pub fn source(connection: &Connection, action: &str) -> String {
    let named = connection.label.trim();
    if named.is_empty() { action.to_owned() } else { format!("{named} {action}") }
}

impl Connectors {
    pub fn labelled(&self, connection: &Connection, action: &str, value: Value) -> Labelled {
        let Ok(entry) = self.entry(&connection.provider) else {
            return Labelled { read: Read::new(source(connection, action), Trust::External, None), value };
        };
        let spec = entry.descriptor.action(action);
        let trust = spec.map_or(Trust::External, crate::ActionSpec::trusted);
        let declared = spec.and_then(|spec| spec.output_level.clone());
        let label = entry.provider.labelled(action, &value).or(declared);
        let sensitive = spec.is_some_and(|spec| spec.profile.sensitive) || self.holds_a_secret(&value);
        let acted = spec.is_some_and(|spec| spec.profile.acts());
        let read = Read::new(source(connection, action), trust, label).exposed(sensitive, acted);
        Labelled { read, value }
    }

    pub fn holds_a_secret(&self, value: &Value) -> bool {
        !self.redactor().redact_value(value, &domain::redact::Known::default()).1.is_empty()
    }

    pub async fn call_labelled(
        &self,
        attribution: Option<&Attribution>,
        connection: &Connection,
        settings: &Value,
        action: &str,
        args: &Value,
        idempotency_key: &str,
    ) -> Result<Labelled, ActionError> {
        let value = self.call_for(attribution, connection, settings, action, args, idempotency_key).await?;
        Ok(self.labelled(connection, action, value))
    }
}
