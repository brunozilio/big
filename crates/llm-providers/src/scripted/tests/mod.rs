mod antigravity;
mod chatgpt;
mod chatgpt_client;
mod claude;
mod copilot;
mod copilot_client;
mod grok;
mod grok_client;
mod kiro;
mod kiro_client;
mod kiro_sign_in;
#[cfg(feature = "local-models")]
mod llamacpp;
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
    assert_eq!(provider("claude-subscription").later_discount(), None);
}
