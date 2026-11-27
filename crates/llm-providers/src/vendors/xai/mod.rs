use crate::Vendor as Family;

#[derive(Debug, Clone, Copy, Default)]
pub struct Vendor;

impl Family for Vendor {
    fn id(&self) -> &'static str {
        "xai"
    }

    fn owns(&self, model: &str) -> bool {
        model.to_ascii_lowercase().starts_with("grok")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_owns_grok_models_only() {
        assert!(Vendor.owns("grok-4.6") && !Vendor.owns("gpt-5"));
    }
}
