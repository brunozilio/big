use domain::ModelOption;
use domain::model::ModelCapabilities;
use serde_json::{Value, json};

use llm_providers::EFFORTS;

const FIELDS: [&str; 2] = ["output_config", "reasoning"];

pub fn declared(schema: &Value) -> Option<ModelCapabilities> {
    FIELDS.iter().find_map(|field| {
        let levels: Vec<String> = schema["properties"][field]["properties"]["effort"]["enum"]
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        (!levels.is_empty()).then(|| capabilities(*field == "output_config", levels))
    })
}

pub fn known(model: &str) -> Option<ModelCapabilities> {
    let rest = model.strip_prefix("claude-")?;
    let version = ["opus-", "sonnet-"].iter().find_map(|family| rest.strip_prefix(family))?;
    let mut parts = version.splitn(2, '.');
    let major = number(parts.next()?)?;
    let minor = parts.next().map_or(Some(0), number)?;
    let levels: &[&str] = match (major, minor) {
        (4, 6) => &["low", "medium", "high", "max"],
        (4, 7..) | (5.., _) => &EFFORTS,
        _ => return None,
    };
    Some(capabilities(true, levels.iter().map(|level| (*level).to_owned()).collect()))
}

pub fn put(model: Option<&ModelOption>, body: &mut Value, effort: &str) {
    llm_providers::request::remove(body, &["output_config"]);
    let Some(model) = model else { return };
    let bare = model.id.rsplit(':').next().unwrap_or(&model.id);
    let listed = model.capabilities.clone().filter(|caps| !caps.reasoning_levels.is_empty());
    let Some(caps) = listed.or_else(|| known(bare)) else { return };
    if bare == "auto" {
        return;
    }
    let field = if caps.adaptive_thinking { "output_config" } else { "reasoning" };
    body["additionalModelRequestFields"] = json!({ field: { "effort": pick(&caps.reasoning_levels, effort) } });
}

fn pick(levels: &[String], effort: &str) -> String {
    let asked = EFFORTS.iter().position(|level| *level == effort).unwrap_or(1);
    EFFORTS[..=asked]
        .iter()
        .rev()
        .find(|level| levels.iter().any(|given| given == *level))
        .map(|level| (*level).to_owned())
        .or_else(|| levels.iter().find(|given| EFFORTS.contains(&given.as_str())).cloned())
        .or_else(|| levels.first().cloned())
        .unwrap_or_default()
}

fn capabilities(adaptive: bool, levels: Vec<String>) -> ModelCapabilities {
    ModelCapabilities {
        reasoning: true,
        adaptive_thinking: adaptive,
        reasoning_levels: levels,
        ..ModelCapabilities::default()
    }
}

fn number(text: &str) -> Option<u32> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude(id: &str, caps: Option<ModelCapabilities>) -> ModelOption {
        let mut model = ModelOption::new(format!("kiro-subscription:{id}"), id);
        model.capabilities = caps;
        model
    }

    fn sent(model: &ModelOption, effort: &str) -> Value {
        let mut body = json!({"conversationState": {}, "output_config": {"effort": effort}});
        put(Some(model), &mut body, effort);
        assert!(body.get("output_config").is_none());
        body["additionalModelRequestFields"].clone()
    }

    #[test]
    fn each_effort_goes_to_the_output_config_of_a_claude_model() {
        let model = claude("claude-opus-4.8", known("claude-opus-4.8"));
        for effort in EFFORTS {
            assert_eq!(sent(&model, effort), json!({"output_config": {"effort": effort}}), "{effort}");
        }
    }

    #[test]
    fn an_effort_the_model_lacks_falls_to_the_next_one_below() {
        let model = claude("claude-sonnet-4.6", None);
        assert_eq!(sent(&model, "xhigh"), json!({"output_config": {"effort": "high"}}));
        assert_eq!(sent(&model, "max"), json!({"output_config": {"effort": "max"}}));
    }

    #[test]
    fn the_schema_of_the_catalog_names_the_field_and_the_levels() {
        let gpt = declared(&json!({"properties": {"reasoning": {"properties": {"effort": {
            "enum": ["none", "low", "medium", "high", "xhigh", "max"]
        }}}}}));
        let model = claude("gpt-5.6-sol", gpt);
        assert_eq!(sent(&model, "low"), json!({"reasoning": {"effort": "low"}}));
        let opus = declared(&json!({"properties": {
            "output_config": {"properties": {"effort": {"enum": ["low", "high"]}}},
            "thinking": {"properties": {"type": {"enum": ["adaptive", "disabled"]}}}
        }}));
        assert_eq!(sent(&claude("claude-opus-5", opus), "medium"), json!({"output_config": {"effort": "low"}}));
        assert!(declared(&json!({"properties": {}})).is_none());
    }

    #[test]
    fn models_without_effort_controls_get_no_extra_fields() {
        for id in ["claude-sonnet-4.5", "claude-haiku-4.5", "auto", "glm-5", "claude-opus-4.1"] {
            assert_eq!(sent(&claude(id, None), "high"), Value::Null, "{id}");
        }
        let mut body = json!({"output_config": {"effort": "high"}});
        put(None, &mut body, "high");
        assert_eq!(body, json!({}));
    }

    #[test]
    fn the_known_claude_families_read_their_version() {
        let levels = |id: &str| known(id).map(|caps| caps.reasoning_levels).unwrap_or_default();
        assert_eq!(levels("claude-opus-4.6"), ["low", "medium", "high", "max"]);
        assert_eq!(levels("claude-opus-5"), EFFORTS);
        assert_eq!(levels("claude-sonnet-5.5"), EFFORTS);
        assert!(levels("claude-sonnet-4.5").is_empty() && levels("claude-haiku-5").is_empty());
    }
}
