use domain::ModelOption;
use domain::billing::TokenRates;

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
