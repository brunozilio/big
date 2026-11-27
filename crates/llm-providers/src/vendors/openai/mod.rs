use crate::Vendor as Family;

#[derive(Debug, Clone, Copy, Default)]
pub struct Vendor;

impl Family for Vendor {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn owns(&self, model: &str) -> bool {
        model.starts_with("gpt-") || ["o1", "o3", "o4"].iter().any(|prefix| model.starts_with(prefix))
    }

    fn small(&self) -> Option<&'static str> {
        Some("gpt-6-luna")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_models_are_owned() {
        assert!(Vendor.owns("o3-mini") && !Vendor.owns("claude-opus-5-5"));
    }
}
