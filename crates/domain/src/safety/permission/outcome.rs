use serde::{Deserialize, Serialize};

use super::{BLOCKED, Reason};
use crate::audit::code;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskOutcome {
    Declined,
    Expired,
    BlockedByPolicy,
    Unavailable,
}

impl AskOutcome {
    pub const ALL: [Self; 4] = [Self::Declined, Self::Expired, Self::BlockedByPolicy, Self::Unavailable];

    pub fn word(self) -> &'static str {
        match self {
            Self::Declined => "declined",
            Self::Expired => "expired",
            Self::BlockedByPolicy => BLOCKED,
            Self::Unavailable => "unavailable",
        }
    }

    pub fn named(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|outcome| outcome.word() == word.trim())
    }

    pub fn of_code(decision: &str) -> Option<Self> {
        match decision {
            code::REFUSED => Some(Self::Declined),
            code::ABANDONED => Some(Self::Expired),
            code::UNATTENDED | code::FLOW_UNATTENDED => Some(Self::Unavailable),
            denied if denied.ends_with(".denied") => Some(Self::BlockedByPolicy),
            _ => None,
        }
    }

    pub fn refusal(self, reason: &str) -> String {
        format!("{}: {reason}", self.word())
    }
}

impl Reason {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Mode => "mode",
            Self::Hook => "hook",
            Self::Listed => "listed",
            Self::Flagged(_) => "flagged",
            Self::Declared { .. } => "declared",
            Self::Outside(_) => "outside",
            Self::Guarded { .. } => "guarded",
            Self::Hard(_) => "hard",
            Self::Flow(_) => "flow",
        }
    }

    pub fn why(&self) -> Option<String> {
        match self {
            Self::Flagged(why) | Self::Flow(why) | Self::Declared { why, .. } => Some(why.clone()),
            Self::Hard(category) => Some(category.why().to_owned()),
            Self::Guarded { guard, .. } => Some(guard.why().to_owned()),
            Self::Outside(path) => Some(path.display().to_string()),
            Self::Mode | Self::Hook | Self::Listed => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::hard::Category;

    #[test]
    fn every_outcome_has_a_word_of_its_own_and_reads_back() {
        let words: Vec<_> = AskOutcome::ALL.iter().map(|outcome| outcome.word()).collect();
        assert_eq!(words, ["declined", "expired", "blocked_by_policy", "unavailable"]);
        for outcome in AskOutcome::ALL {
            assert_eq!(AskOutcome::named(outcome.word()), Some(outcome));
            let json = serde_json::to_value(outcome).unwrap_or_default();
            assert_eq!(json, outcome.word(), "the wire word is the same");
        }
        assert_eq!(AskOutcome::named("allowed"), None);
    }

    #[test]
    fn the_existing_decision_codes_name_the_outcome() {
        assert_eq!(AskOutcome::of_code(code::REFUSED), Some(AskOutcome::Declined));
        assert_eq!(AskOutcome::of_code(code::ABANDONED), Some(AskOutcome::Expired));
        assert_eq!(AskOutcome::of_code(code::UNATTENDED), Some(AskOutcome::Unavailable));
        assert_eq!(AskOutcome::of_code(code::FLOW_UNATTENDED), Some(AskOutcome::Unavailable));
        assert_eq!(AskOutcome::of_code(&code::denied("floor")), Some(AskOutcome::BlockedByPolicy));
        assert_eq!(AskOutcome::of_code(code::GRANTED), None);
        assert_eq!(AskOutcome::of_code(code::STANDING), None);
    }

    #[test]
    fn a_refusal_starts_with_the_outcome_and_a_reason_gives_its_facts() {
        assert_eq!(AskOutcome::Unavailable.refusal("x"), "unavailable: x");
        assert_eq!(AskOutcome::BlockedByPolicy.refusal("x"), "blocked_by_policy: x");
        let hard = Reason::Hard(Category::Credentials);
        assert_eq!((hard.kind(), hard.why().as_deref()), ("hard", Some(Category::Credentials.why())));
        assert_eq!((Reason::Listed.kind(), Reason::Listed.why()), ("listed", None));
    }
}
