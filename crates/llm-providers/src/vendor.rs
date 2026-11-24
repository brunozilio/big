use domain::ModelOption;
use domain::billing::TokenRates;
use serde_json::Value;

use super::ProviderError;

pub trait Vendor: Sync {
    fn id(&self) -> &'static str;
    fn owns(&self, model: &str) -> bool;
    fn rates(&self, model: &str, prompt_tokens: u64) -> Option<TokenRates>;

    fn search_price(&self) -> Option<u64> {
        None
    }

    fn alias(&self, _name: &str) -> Option<&'static str> {
        None
    }

    fn small(&self) -> Option<&'static str> {
        None
    }

    fn reasons(&self, _model: &str) -> bool {
        false
    }

    fn efforts(&self, model: &str) -> Vec<&'static str> {
        if self.reasons(model) { EFFORTS.to_vec() } else { Vec::new() }
    }

    fn effort(
        &self,
        _body: &mut Value,
        _selected: Option<&ModelOption>,
        _effort: &str,
        _always: bool,
    ) -> Result<(), ProviderError> {
        Ok(())
    }
}

pub fn vendor(id: &str) -> Option<&'static dyn Vendor> {
    crate::vendors::ALL.iter().copied().find(|vendor| vendor.id() == id)
}

pub fn owner(model: &str) -> Option<&'static dyn Vendor> {
    let model = bare(model);
    crate::vendors::ALL.iter().copied().find(|vendor| vendor.owns(model))
}

pub fn rates(vendor_id: &str, model: &str, prompt_tokens: u64) -> Option<TokenRates> {
    let model = bare(model);
    vendor(vendor_id).filter(|vendor| vendor.owns(model)).and_then(|vendor| vendor.rates(model, prompt_tokens))
}

pub fn priced(provider: &str, vendor_id: &str, model: &str, prompt_tokens: u64) -> Option<TokenRates> {
    match crate::prices::source(provider) {
        Some(sources) => {
            sources.split_whitespace().find_map(|source| crate::prices::listed(source, model, prompt_tokens))
        }
        None => rates(vendor_id, model, prompt_tokens),
    }
}

pub fn search_price(vendor_id: &str) -> Option<u64> {
    vendor(vendor_id).and_then(Vendor::search_price)
}

pub fn effort(
    vendor_id: &str,
    body: &mut Value,
    selected: Option<&ModelOption>,
    effort: &str,
    always: bool,
) -> Result<(), ProviderError> {
    vendor(vendor_id).map_or(Ok(()), |vendor| vendor.effort(body, selected, effort, always))
}

pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub fn efforts(model: &ModelOption) -> Vec<&'static str> {
    let caps = model.capabilities.as_ref();
    let offered = caps.map(|caps| caps.reasoning_levels.as_slice()).unwrap_or_default();
    if !offered.is_empty() {
        return EFFORTS.into_iter().filter(|level| offered.iter().any(|given| given == level)).collect();
    }
    let name = bare(&model.id);
    let known = owner(name).map(|vendor| vendor.efforts(name)).unwrap_or_default();
    if known.is_empty() && caps.is_some_and(|caps| caps.reasoning || caps.adaptive_thinking) {
        return EFFORTS.to_vec();
    }
    known
}

fn bare(model: &str) -> &str {
    model.rsplit(':').next().unwrap_or(model)
}
