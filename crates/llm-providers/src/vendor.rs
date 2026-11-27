use domain::ModelOption;
use domain::billing::TokenRates;

use crate::Provider;
use crate::model::Cache;

pub trait Vendor: Sync {
    fn id(&self) -> &'static str;
    fn owns(&self, model: &str) -> bool;

    fn alias(&self, _name: &str) -> Option<&'static str> {
        None
    }

    fn small(&self) -> Option<&'static str> {
        None
    }

}

pub fn vendor(id: &str) -> Option<&'static dyn Vendor> {
    crate::vendors::ALL.iter().copied().find(|vendor| vendor.id() == id)
}

pub fn owner(model: &str) -> Option<&'static dyn Vendor> {
    let model = bare(model);
    crate::vendors::ALL.iter().copied().find(|vendor| vendor.owns(model))
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
