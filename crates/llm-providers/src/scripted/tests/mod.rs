mod antigravity;
mod antigravity_wire;
mod chatgpt;
mod chatgpt_client;
mod claude;
mod claude_batch;
mod copilot;
mod copilot_client;
mod grok;
mod grok_client;
mod kiro;
mod kiro_client;
mod kiro_sign_in;
#[cfg(feature = "local-models")]
mod llamacpp;
mod ownership;
mod prices;
mod smoke;

use domain::ModelProvider;

use crate::Provider;

fn provider(id: &'static str) -> &'static dyn Provider {
    crate::providers::get(ModelProvider::new(id)).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn a_provider_declares_the_discount_of_its_low_priority_mode_and_none_is_the_default() {
    let root = std::env::temp_dir().join(format!("later-discount-{}", uuid::Uuid::new_v4()));
    let folder = root.join("providers/cheap");
    std::fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("{error}"));
    let declared = r#"export function provider() { return { id: "cheap", name: "Cheap", laterDiscount: 0.5 }; }"#;
    std::fs::write(folder.join("index.ts"), declared).unwrap_or_else(|error| panic!("{error}"));
    let loaded = crate::scripted::ScriptedProvider::load_in(Some(&root), "providers/cheap/index.ts");
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(loaded.unwrap_or_else(|error| panic!("{error}")).later_discount(), Some(0.5));
    assert_eq!(provider("grok-subscription").later_discount(), None);
}

#[test]
fn a_provider_declares_where_and_how_it_keeps_data_and_a_model_may_say_otherwise() {
    let root = std::env::temp_dir().join(format!("handling-{}", uuid::Uuid::new_v4()));
    let folder = root.join("providers/regional");
    std::fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("{error}"));
    let declared = r#"export function provider() { return { id: "regional", name: "Regional", region: "us-east-1", dataHandling: { training: "no", retention: 30 } }; }
export function handling({ name }) { return name.startsWith("eu-") ? { region: "eu-west-1" } : null; }"#;
    std::fs::write(folder.join("index.ts"), declared).unwrap_or_else(|error| panic!("{error}"));
    let loaded = crate::scripted::ScriptedProvider::load_in(Some(&root), "providers/regional/index.ts");
    let _ = std::fs::remove_dir_all(&root);
    let loaded = loaded.unwrap_or_else(|error| panic!("{error}"));
    let us = domain::data_policy::Handling {
        region: Some("us-east-1".into()),
        training: Some(false),
        retention_days: Some(30),
    };
    assert_eq!(loaded.handling("m"), us);
    assert_eq!(loaded.handling("eu-m"), domain::data_policy::Handling { region: Some("eu-west-1".into()), ..us });
}

#[test]
fn the_shipped_remote_providers_do_not_claim_a_region_or_retention_they_do_not_know() {
    for id in [
        "claude-subscription",
        "openai-subscription",
        "grok-subscription",
        "copilot-subscription",
        "kiro-subscription",
        "antigravity-subscription",
    ] {
        assert_eq!(provider(id).handling("any"), domain::data_policy::Handling::default(), "{id}");
    }
}

#[test]
fn each_provider_declares_its_own_native_web_search_per_model() {
    let claude = provider("claude-subscription");
    let filtering = serde_json::json!({ "type": "web_search_20260209", "name": "web_search" });
    let basic = serde_json::json!({ "type": "web_search_20250305", "name": "web_search" });
    for model in ["claude-opus-5-5", "claude-sonnet-4-6", "claude-fable-5-1", "claude-opus-4-6"] {
        assert_eq!(claude.web_search(model), Some(filtering.clone()), "{model}");
    }
    for model in ["claude-haiku-4-5", "claude-opus-4-5", "claude-sonnet-4-5"] {
        assert_eq!(claude.web_search(model), Some(basic.clone()), "{model}");
    }
    for id in ["openai-subscription", "grok-subscription"] {
        assert_eq!(provider(id).web_search("any"), Some(serde_json::json!({ "type": "web_search" })), "{id}");
    }
    for id in ["copilot-subscription", "antigravity-subscription"] {
        assert_eq!(provider(id).web_search("claude-opus-5-5"), None, "{id}");
    }
}
