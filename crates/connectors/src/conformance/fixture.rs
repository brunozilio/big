use serde::Deserialize;
use serde_json::{Map, Value};

pub const FILE: &str = "conformance.json";
pub const VERSION: u64 = 1;
pub const ACCOUNTS: &str = "accounts";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub schema_version: u64,
    #[serde(default)]
    pub creds: Given,
    #[serde(default)]
    pub config: Map<String, Value>,
    pub cases: Vec<Case>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Given {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub settings: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub name: String,
    pub action: String,
    #[serde(default)]
    pub args: Value,
    #[serde(default)]
    pub creds: Option<Given>,
    #[serde(default)]
    pub http: Vec<Answer>,
    pub expect: Expect,
    #[serde(default)]
    pub sent: Vec<String>,
    #[serde(default)]
    pub next: Option<Next>,
    #[serde(default = "typed")]
    pub typed: bool,
}

fn typed() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub method: String,
    pub url: String,
    #[serde(default = "success")]
    pub status: u16,
    #[serde(default)]
    pub body: Value,
    #[serde(default)]
    pub headers: Map<String, Value>,
    #[serde(default)]
    pub repeat: bool,
}

fn success() -> u16 {
    200
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    #[serde(default)]
    pub ok: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Next {
    pub cursor: String,
    pub arg: String,
    #[serde(default)]
    pub http: Vec<Answer>,
    pub expect: Expect,
}

pub fn parsed(text: &str) -> Result<Fixture, String> {
    let fixture: Fixture = serde_json::from_str(text).map_err(|error| format!("{FILE} does not read: {error}"))?;
    if fixture.schema_version != VERSION {
        return Err(format!("{FILE} declares schema_version {}, the harness reads {VERSION}", fixture.schema_version));
    }
    if fixture.cases.is_empty() {
        return Err(format!("{FILE} declares no case"));
    }
    for case in &fixture.cases {
        if case.expect.ok.is_some() == case.expect.error.is_some() {
            return Err(format!("{}: expect says either ok or error", case.name));
        }
    }
    Ok(fixture)
}

pub fn contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            expected.iter().all(|(name, wanted)| actual.get(name).is_some_and(|found| contains(found, wanted)))
        }
        (Value::Array(actual), Value::Array(expected)) => {
            actual.len() >= expected.len() && actual.iter().zip(expected).all(|(found, wanted)| contains(found, wanted))
        }
        _ => actual == expected,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_expected_value_is_a_part_of_the_answer() {
        let answer = json!({ "a": 1, "b": [{ "c": 2, "d": 3 }, { "c": 4 }], "e": "x" });
        assert!(contains(&answer, &json!({ "b": [{ "c": 2 }] })));
        assert!(contains(&answer, &json!({})));
        assert!(!contains(&answer, &json!({ "a": 2 })));
        assert!(!contains(&answer, &json!({ "z": null })));
        assert!(!contains(&answer, &json!({ "b": [{}, {}, {}] })));
    }

    #[test]
    fn a_fixture_says_its_version_and_one_outcome_per_case() {
        let case =
            |expect: Value| json!({ "schema_version": 1, "cases": [{ "name": "n", "action": "a", "expect": expect }] });
        assert!(parsed(&case(json!({ "ok": {} })).to_string()).is_ok());
        assert!(parsed(&case(json!({})).to_string()).is_err());
        assert!(parsed(&case(json!({ "ok": {}, "error": "auth" })).to_string()).is_err());
        assert!(parsed(&json!({ "schema_version": 2, "cases": [] }).to_string()).is_err());
        assert!(parsed(&json!({ "schema_version": 1, "cases": [] }).to_string()).is_err());
        assert!(parsed("{ \"schema_version\": 1, \"cases\": [], \"extra\": 1 }").is_err());
    }
}
