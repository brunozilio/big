use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::redact::Rules;

pub const CONTENT_FIELDS: &[&str] = &[
    "gen_ai.tool.call.arguments",
    "gen_ai.tool.call.result",
    "gen_ai.input.messages",
    "gen_ai.output.messages",
    "prompt.text",
];

static DROPPED: AtomicU64 = AtomicU64::new(0);

pub fn dropped() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

pub fn drop_one() {
    DROPPED.fetch_add(1, Ordering::Relaxed);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keys {
    pub usage: bool,
    pub content: bool,
    pub prompts: bool,
}

impl Default for Keys {
    fn default() -> Self {
        Keys { usage: true, content: false, prompts: false }
    }
}

fn on(text: &str, otherwise: bool) -> bool {
    match text.trim() {
        "true" => true,
        "false" => false,
        _ => otherwise,
    }
}

impl Keys {
    pub fn read(usage: &str, content: &str, prompts: &str) -> Keys {
        let default = Keys::default();
        Keys {
            usage: on(usage, default.usage),
            content: on(content, default.content),
            prompts: on(prompts, default.prompts),
        }
    }

    pub fn configured() -> Keys {
        static KEYS: OnceLock<Keys> = OnceLock::new();
        *KEYS.get_or_init(|| {
            Keys::read(brand::traces::CAPTURE_USAGE, brand::traces::CAPTURE_CONTENT, brand::traces::CAPTURE_PROMPTS)
        })
    }

    pub fn shared(self, sharing: Sharing) -> Keys {
        match sharing {
            Sharing::Full => self,
            Sharing::Metadata => Keys { content: false, prompts: false, ..self },
            Sharing::Disabled => Keys { usage: false, content: false, prompts: false },
        }
    }

    pub fn any_text(self) -> bool {
        self.content || self.prompts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sharing {
    #[default]
    Full,
    Metadata,
    Disabled,
}

impl Sharing {
    pub fn read(text: &str) -> Sharing {
        match text.trim() {
            "metadata" => Sharing::Metadata,
            "disabled" => Sharing::Disabled,
            "full" | "" => Sharing::Full,
            _ => Sharing::Metadata,
        }
    }

    pub fn ignored(text: &str) -> Option<&str> {
        let text = text.trim();
        (!matches!(text, "" | "full" | "metadata" | "disabled")).then_some(text)
    }

    pub fn carries_content(self) -> bool {
        self == Sharing::Full
    }
}

pub fn revision(keys: Keys, sharing: Sharing, rules: &Rules) -> String {
    let policy = format!("{:?}|{sharing:?}|{rules:?}", keys.shared(sharing));
    let digest = Sha256::digest(policy.as_bytes());
    digest.iter().take(8).map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_usage_is_captured_unless_asked() {
        assert_eq!(Keys::read("", "", ""), Keys { usage: true, content: false, prompts: false });
        assert_eq!(Keys::read("false", "true", "true"), Keys { usage: false, content: true, prompts: true });
        assert_eq!(Keys::read("yes", "1", "on"), Keys::default());
    }

    #[test]
    fn a_sink_shares_no_more_than_it_declares() {
        let all = Keys { usage: true, content: true, prompts: true };
        assert_eq!(all.shared(Sharing::Full), all);
        assert_eq!(all.shared(Sharing::Metadata), Keys::default());
        assert!(!all.shared(Sharing::Disabled).usage);
        assert_eq!(Sharing::read(" metadata "), Sharing::Metadata);
        assert_eq!(Sharing::read(""), Sharing::Full);
        assert_eq!(Sharing::read("everything"), Sharing::Metadata);
        assert_eq!(Sharing::ignored(" everything "), Some("everything"));
        assert_eq!(Sharing::ignored("disabled"), None);
    }

    #[test]
    fn the_revision_follows_the_policy_and_the_rules() {
        let rules = Rules::default();
        let first = revision(Keys::default(), Sharing::Full, &rules);
        assert_eq!(first.len(), 16);
        assert_eq!(first, revision(Keys::default(), Sharing::Full, &rules));
        assert_ne!(first, revision(Keys { content: true, ..Keys::default() }, Sharing::Full, &rules));
        let wider = Rules { labels: vec!["bearer".into()], ..Rules::default() };
        assert_ne!(first, revision(Keys::default(), Sharing::Full, &wider));
        assert_eq!(
            revision(Keys { content: true, ..Keys::default() }, Sharing::Metadata, &rules),
            revision(Keys::default(), Sharing::Metadata, &rules)
        );
    }
}
