use domain::ModelOption;
use domain::billing::TokenRates;

use crate::Provider;
use crate::model::Cache;

pub fn owns<P: Provider + ?Sized>(provider: &P, model: &str) -> bool {
    let model = bare(model).to_ascii_lowercase();
    provider.owns().iter().any(|prefix| model.starts_with(&prefix.to_ascii_lowercase()))
}

pub fn owner(model: &str) -> Option<&'static dyn Provider> {
    crate::providers::all().into_iter().find(|provider| owns(*provider, model))
}

pub fn serves<P: Provider + ?Sized>(provider: &P, model: &str) -> bool {
    if provider.owns().is_empty() { owner(model).is_some() } else { owns(provider, model) }
}

pub fn maker(model: &str) -> Option<&'static str> {
    let id = model.to_ascii_lowercase();
    let words: Vec<&str> = id.split(|letter: char| !letter.is_ascii_alphanumeric()).collect();
    let providers = crate::providers::all();
    let mut makers = providers.iter().flat_map(|provider| provider.makers().iter());
    makers
        .find(|(word, _)| words.iter().any(|said| said.starts_with(&word.to_ascii_lowercase())))
        .map(|(_, logo)| *logo)
}

pub fn priced(provider: &dyn Provider, model: &str, prompt_tokens: u64, cache: Cache) -> Option<TokenRates> {
    crate::prices::overridden(provider.id(), model, prompt_tokens)
        .or_else(|| provider.prices().rates(model, prompt_tokens, cache))
}

pub fn later_discount(provider: &dyn Provider, model: &str) -> Option<f64> {
    let declared = provider.prices().price(model).and_then(|price| price.later_discount);
    declared.or_else(|| provider.later_discount()).filter(|discount| (0.0..=1.0).contains(discount))
}

pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub fn efforts(model: &ModelOption) -> Vec<&'static str> {
    let Some(caps) = model.capabilities.as_ref() else { return Vec::new() };
    if !caps.reasoning_levels.is_empty() {
        return EFFORTS.into_iter().filter(|level| caps.reasoning_levels.iter().any(|given| given == level)).collect();
    }
    if caps.reasoning || caps.adaptive_thinking { EFFORTS.to_vec() } else { Vec::new() }
}

fn bare(model: &str) -> &str {
    model.rsplit(':').next().unwrap_or(model)
}

#[cfg(test)]
mod tests {
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
