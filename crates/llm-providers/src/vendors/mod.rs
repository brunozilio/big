pub mod anthropic;
pub mod openai;
pub mod xai;

pub static ALL: &[&dyn crate::Vendor] = &[&anthropic::Vendor, &openai::Vendor, &xai::Vendor];

#[cfg(test)]
mod tests {
    use super::ALL;

    #[test]
    fn the_registered_vendors_are_these_in_this_order() {
        let ids: Vec<&str> = ALL.iter().map(|vendor| vendor.id()).collect();
        assert_eq!(ids, ["anthropic", "openai", "xai"]);
    }

    #[test]
    fn each_family_offers_the_efforts_its_requests_take() {
        use domain::ModelOption;
        use domain::model::ModelCapabilities;
        let named = |id: &str| crate::efforts(&ModelOption::new(id, id));
        assert_eq!(named("anthropic:claude-opus-5-5"), crate::EFFORTS);
        assert_eq!(named("openai:gpt-6-astra"), ["low", "medium", "high", "xhigh"]);
        assert_eq!(named("xai:grok-4.3"), ["low", "medium", "high"]);
        assert!(named("xai:grok-2").is_empty() && named("other:plain").is_empty());
        let mut listed = ModelOption::new("copilot:x", "x");
        listed.capabilities =
            Some(ModelCapabilities { reasoning_levels: vec!["low".into(), "high".into()], ..Default::default() });
        assert_eq!(crate::efforts(&listed), ["low", "high"]);
    }
}
