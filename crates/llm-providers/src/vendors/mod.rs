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
    fn the_efforts_offered_are_what_the_catalog_declares() {
        use domain::ModelOption;
        use domain::model::ModelCapabilities;
        let with = |caps: Option<ModelCapabilities>| {
            let mut model = ModelOption::new("x:claude-opus-5-5", "x");
            model.capabilities = caps;
            crate::efforts(&model)
        };
        assert!(with(None).is_empty(), "no vendor is guessed from the model's name");
        let levels = ModelCapabilities { reasoning_levels: vec!["high".into(), "low".into()], ..Default::default() };
        assert_eq!(with(Some(levels)), ["low", "high"]);
        assert_eq!(with(Some(ModelCapabilities { reasoning: true, ..Default::default() })), crate::EFFORTS);
        assert_eq!(with(Some(ModelCapabilities { adaptive_thinking: true, ..Default::default() })), crate::EFFORTS);
        assert!(with(Some(ModelCapabilities::default())).is_empty());
    }
}
