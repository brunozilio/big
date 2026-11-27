use super::provider;

#[test]
fn each_maker_declares_the_models_it_owns_and_a_reseller_serves_any_owned_one() {
    let owner = |model: &str| crate::owner(model).map(|found| found.id());
    assert_eq!(owner("claude-opus-5-5"), Some("claude-subscription"));
    assert_eq!(owner("copilot-subscription:gpt-6.1-sol"), Some("openai-subscription"));
    assert_eq!(owner("o3-mini"), Some("openai-subscription"));
    assert_eq!(owner("Grok-4.6"), Some("grok-subscription"));
    assert_eq!(owner("gemini-3.8-flash"), None);
    let claude = provider("claude-subscription");
    assert!(claude.serves("claude-haiku-5-5") && !claude.serves("gpt-5"));
    let chatgpt = provider("openai-subscription");
    assert!(chatgpt.serves("o4-mini") && !chatgpt.serves("claude-opus-5-5"));
    for reseller in ["copilot-subscription", "kiro-subscription"] {
        let reseller = provider(reseller);
        assert!(reseller.serves("claude-sonnet-4.5") && reseller.serves("gpt-6.1-sol") && reseller.serves("grok-4"));
        assert!(!reseller.serves("gemini-3.8-flash") && !reseller.serves("unknown-model"));
    }
    assert!(provider("antigravity-subscription").serves("anything"));
}

#[test]
fn short_names_and_the_small_model_are_what_each_provider_declares() {
    let claude = provider("claude-subscription");
    assert_eq!(claude.alias("opus"), Some("claude-opus-5-5"));
    assert_eq!(claude.alias("sonnet"), Some("claude-sonnet-5-5"));
    assert_eq!(claude.alias("haiku"), Some("claude-haiku-5-5"));
    assert_eq!(claude.alias("gpt"), None);
    assert_eq!(provider("kiro-subscription").alias("haiku"), None);
    assert_eq!(claude.small_model(), Some("claude-haiku-5-5"));
    assert_eq!(provider("openai-subscription").small_model(), Some("gpt-6-luna"));
    assert_eq!(provider("kiro-subscription").small_model(), Some("claude-haiku-4.5"));
    for id in ["grok-subscription", "copilot-subscription", "antigravity-subscription"] {
        assert_eq!(provider(id).small_model(), None, "{id}");
    }
}

#[test]
fn the_logo_of_a_model_is_its_declared_maker_whatever_account_serves_it() {
    for (model, logo) in [
        ("claude-sonnet-4-5", Some("claude")),
        ("gpt-5", Some("openai")),
        ("gpt-5-codex", Some("openai")),
        ("o3-mini", Some("openai")),
        ("grok-4", Some("grok")),
        ("gemini-3-pro", Some("google")),
        ("us.anthropic.claude-opus-4", Some("claude")),
        ("auto-kiro", None),
    ] {
        assert_eq!(crate::maker(model), logo, "{model}");
    }
}
