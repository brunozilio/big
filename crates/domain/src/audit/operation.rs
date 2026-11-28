use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationTag {
    pub id: String,
    #[serde(default, deserialize_with = "version_of", skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friendly_name: Option<String>,
}

fn version_of<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(text) => Some(text.trim().to_owned()).filter(|text| !text.is_empty()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

impl OperationTag {
    pub fn plain(id: &str) -> Self {
        Self { id: id.to_owned(), version: None, friendly_name: None }
    }

    pub fn read(declared: &Value) -> Option<Self> {
        serde_json::from_value::<Self>(declared.clone()).ok().filter(|tag| !tag.id.trim().is_empty())
    }

    pub fn label(&self) -> String {
        match &self.version {
            Some(version) => format!("{}@{version}", self.id),
            None => self.id.clone(),
        }
    }
}
