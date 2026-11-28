use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::item::StepRun;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Audience {
    #[default]
    Me,
    Group(String),
    Space(String),
}

impl TryFrom<String> for Audience {
    type Error = String;

    fn try_from(text: String) -> Result<Audience, String> {
        let wanted = text.trim();
        if wanted == "me" {
            return Ok(Audience::Me);
        }
        let named = |prefix: &str| wanted.strip_prefix(prefix).map(str::trim).filter(|name| !name.is_empty());
        if let Some(group) = named("group:") {
            return Ok(Audience::Group(group.to_owned()));
        }
        if let Some(space) = named("space:") {
            return Ok(Audience::Space(space.to_owned()));
        }
        Err(format!("output audience {wanted:?} is none of me, group:<name> or space:<name>"))
    }
}

impl From<Audience> for String {
    fn from(audience: Audience) -> String {
        match audience {
            Audience::Me => "me".to_owned(),
            Audience::Group(name) => format!("group:{name}"),
            Audience::Space(name) => format!("space:{name}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub kind: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub done: Map<String, Value>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub via: Option<String>,
    #[serde(default)]
    pub template: Value,
    #[serde(default)]
    pub audience: Audience,
    #[serde(default)]
    pub schema_version: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Delivered {
    pub kind: String,
    pub key: String,
    #[serde(default)]
    pub url: Option<String>,
    pub done: bool,
    pub hash: String,
    pub at: i64,
    pub audience: Audience,
}

#[derive(Debug)]
pub struct Found<'a> {
    pub step: &'a StepRun,
    pub key: String,
    pub url: Option<String>,
    pub done: bool,
}

pub fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn digest(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

impl Output {
    pub fn read(value: &Value) -> Result<Option<Output>, String> {
        if value.is_null() {
            return Ok(None);
        }
        let output: Output = serde_json::from_value(value.clone()).map_err(|error| format!("output: {error}"))?;
        if output.kind.trim().is_empty() {
            return Err("output needs a kind".to_owned());
        }
        Ok(Some(output))
    }

    pub fn field(&self) -> &str {
        self.key.as_deref().unwrap_or(&self.kind)
    }

    fn link(&self) -> &str {
        self.url.as_deref().unwrap_or("url")
    }

    fn finished(&self, output: &Value, key: &str) -> bool {
        text(&output[self.field()]).as_deref() == Some(key)
            && self.done.iter().all(|(field, wanted)| &output[field.as_str()] == wanted)
    }

    pub fn found<'a>(&self, steps: &'a [StepRun]) -> Option<Found<'a>> {
        let step = steps.iter().rev().find(|run| text(&run.output[self.field()]).is_some())?;
        let key = text(&step.output[self.field()])?;
        let opened = steps.iter().find(|run| text(&run.output[self.field()]).as_deref() == Some(&key));
        let url = opened.into_iter().chain([step]).find_map(|run| run.output[self.link()].as_str().map(str::to_owned));
        let done = steps.iter().any(|run| self.finished(&run.output, &key));
        Some(Found { step, key, url, done })
    }

    pub fn deliver(&self, steps: &[StepRun], at: i64) -> Option<Delivered> {
        let found = self.found(steps)?;
        Some(Delivered {
            kind: self.kind.clone(),
            hash: digest(&found.step.output),
            key: found.key,
            url: found.url,
            done: found.done,
            at,
            audience: self.audience.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ran(id: &str, output: Value) -> StepRun {
        StepRun { id: id.into(), output, ..StepRun::default() }
    }

    fn declared(value: Value) -> Output {
        Output::read(&value).expect("valid").expect("declared")
    }

    #[test]
    fn the_audience_is_me_a_group_or_a_space_and_nothing_else() {
        let read = |text: &str| Audience::try_from(text.to_owned());
        assert_eq!(read("me"), Ok(Audience::Me));
        assert_eq!(read("group:legal"), Ok(Audience::Group("legal".into())));
        assert_eq!(read("space:finance"), Ok(Audience::Space("finance".into())));
        assert!(read("group:").is_err());
        assert!(read("everyone").is_err());
        assert_eq!(String::from(Audience::Group("legal".into())), "group:legal");
        assert!(Output::read(&json!({ "kind": "report", "audience": "public" })).is_err());
        assert!(Output::read(&json!({ "kind": " " })).is_err());
        assert!(Output::read(&json!({ "kind": "report", "extra": 1 })).is_err());
        assert_eq!(Output::read(&Value::Null), Ok(None));
    }

    #[test]
    fn an_ending_recorded_before_the_deliverable_existed_still_reads() {
        let old: super::super::WorkflowEvent =
            serde_json::from_value(json!({ "type": "finished", "data": { "output": 1 } })).expect("old event");
        assert_eq!(old, super::super::WorkflowEvent::Finished { output: json!(1), delivered: None });
        let ending: super::super::Ending =
            serde_json::from_value(json!({ "type": "finished", "output": null })).expect("old ending");
        assert_eq!(ending, super::super::Ending::Finished { output: Value::Null, delivered: None });
    }

    #[test]
    fn a_report_is_delivered_by_the_step_that_wrote_it_with_its_hash_and_time() {
        let output = declared(json!({ "kind": "report", "audience": "group:legal" }));
        let steps =
            [ran("draft", json!({ "text": "x" })), ran("write", json!({ "report": "parecer.md", "url": "/f" }))];
        let delivered = output.deliver(&steps, 42).expect("delivered");
        assert_eq!((delivered.key.as_str(), delivered.url.as_deref()), ("parecer.md", Some("/f")));
        assert!(delivered.done);
        assert_eq!(delivered.at, 42);
        assert_eq!(delivered.audience, Audience::Group("legal".into()));
        assert_eq!(delivered.hash.len(), 64);
        let changed = [ran("write", json!({ "report": "parecer.md", "url": "/g" }))];
        assert_ne!(output.deliver(&changed, 42).map(|other| other.hash), Some(delivered.hash));
        assert!(output.deliver(&[ran("draft", json!({}))], 42).is_none());
    }

    #[test]
    fn a_kind_with_a_done_rule_is_done_only_once_a_step_matches_it_for_the_same_key() {
        let output = declared(json!({ "kind": "change", "key": "ref", "url": "link", "done": { "closed": true } }));
        let opened = ran("open", json!({ "ref": "c-7", "link": "https://x/7" }));
        let found = output.deliver(std::slice::from_ref(&opened), 1).expect("found");
        assert!(!found.done);
        let other = ran("note", json!({ "closed": true }));
        assert!(!output.deliver(&[opened.clone(), other], 1).expect("found").done);
        let closed = ran("close", json!({ "ref": "c-7", "closed": true }));
        let delivered = output.deliver(&[opened, closed], 1).expect("found");
        assert!(delivered.done);
        assert_eq!(delivered.url.as_deref(), Some("https://x/7"));
    }
}
