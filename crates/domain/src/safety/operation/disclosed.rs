use serde::{Deserialize, Serialize};

use super::{Kind, Operation, Risk};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Disclosed {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<Risk>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recipients: Vec<String>,
}

impl Disclosed {
    pub fn of<'a>(operations: impl IntoIterator<Item = &'a Operation>) -> Self {
        let mut disclosed = Self::default();
        for operation in operations {
            disclosed.effects.extend(operation.effects.iter().copied());
            for recipient in &operation.recipients {
                if !disclosed.recipients.contains(recipient) {
                    disclosed.recipients.push(recipient.clone());
                }
            }
            disclosed.risk = disclosed.risk.max(Some(operation.risk));
        }
        disclosed.effects.sort();
        disclosed.effects.dedup();
        disclosed
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty() && self.risk.is_none() && self.recipients.is_empty()
    }
}
