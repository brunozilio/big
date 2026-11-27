use crate::Vendor as Family;

#[derive(Debug, Clone, Copy, Default)]
pub struct Vendor;

impl Family for Vendor {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    fn owns(&self, model: &str) -> bool {
        model.starts_with("claude-")
    }

    fn alias(&self, name: &str) -> Option<&'static str> {
        match name {
            "sonnet" => Some("claude-sonnet-5-5"),
            "opus" => Some("claude-opus-5-5"),
            "haiku" => Some("claude-haiku-5-5"),
            _ => None,
        }
    }

    fn small(&self) -> Option<&'static str> {
        Some("claude-haiku-5-5")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_pick_the_current_models() {
        assert_eq!(Vendor.alias("opus"), Some("claude-opus-5-5"));
        assert_eq!(Vendor.alias("gpt"), None);
    }
}
