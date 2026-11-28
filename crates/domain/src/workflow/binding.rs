use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    #[serde(default)]
    pub connection: String,
    pub kind: String,
    #[serde(default)]
    pub value: Value,
}

impl Binding {
    pub const LANE: &'static str = "branch";
    pub const BASE: &'static str = "base";

    pub fn new(connection: &str, kind: &str, value: Value) -> Self {
        Self { connection: connection.to_owned(), kind: kind.to_owned(), value }
    }

    pub fn is(&self, connection: &str, kind: &str) -> bool {
        self.connection == connection && self.kind == kind
    }

    pub fn text(&self) -> Option<&str> {
        self.value.as_str().map(str::trim).filter(|text| !text.is_empty())
    }
}

pub fn bind(bindings: &mut Vec<Binding>, binding: &Binding, keep: bool) {
    match bindings.iter_mut().find(|known| known.is(&binding.connection, &binding.kind)) {
        Some(_) if keep => {}
        Some(known) => known.value = binding.value.clone(),
        None => bindings.push(binding.clone()),
    }
}
