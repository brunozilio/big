use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    #[default]
    Step,
    Hook,
    Trigger,
    Summary,
    Title,
    Filter,
    Review,
    Eval,
}

impl Origin {
    pub const ALL: [Self; 8] =
        [Self::Step, Self::Hook, Self::Trigger, Self::Summary, Self::Title, Self::Filter, Self::Review, Self::Eval];

    pub const fn word(self) -> &'static str {
        match self {
            Self::Step => "step",
            Self::Hook => "hook",
            Self::Trigger => "trigger",
            Self::Summary => "summary",
            Self::Title => "title",
            Self::Filter => "filter",
            Self::Review => "review",
            Self::Eval => "eval",
        }
    }

    pub fn named(word: &str) -> Option<Self> {
        let word = word.trim();
        Self::ALL.into_iter().find(|origin| origin.word().eq_ignore_ascii_case(word))
    }

    pub const fn is_step(&self) -> bool {
        matches!(self, Self::Step)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(default, skip_serializing_if = "Origin::is_step")]
    pub origin: Origin,
}

impl Attribution {
    pub fn of_step(item: impl Into<String>, step: impl Into<String>, origin: Origin) -> Self {
        let step = step.into();
        Self { item: Some(item.into()), step: Some(step).filter(|step| !step.is_empty()), origin }
    }

    pub fn is_unset(&self) -> bool {
        *self == Self::default()
    }

    pub fn as_origin(&self, origin: Origin) -> Self {
        Self { origin, ..self.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_origin_travels_as_its_lowercase_word_and_reads_back_from_it() {
        for origin in Origin::ALL {
            assert_eq!(serde_json::to_value(origin).expect("json"), origin.word());
            assert_eq!(Origin::named(origin.word()), Some(origin));
        }
        assert_eq!(Origin::named(" Summary "), Some(Origin::Summary));
        assert_eq!(Origin::named("chat"), None);
        assert_eq!(Origin::default(), Origin::Step);
    }

    #[test]
    fn an_attribution_names_the_item_and_step_and_keeps_them_under_another_origin() {
        let step = Attribution::of_step("i1", "develop", Origin::Step);
        assert_eq!((step.item.as_deref(), step.step.as_deref()), (Some("i1"), Some("develop")));
        let summary = step.as_origin(Origin::Summary);
        assert_eq!((summary.origin, summary.item.as_deref()), (Origin::Summary, Some("i1")));
        assert_eq!(Attribution::of_step("i1", "", Origin::Eval).step, None);
        let json = serde_json::to_value(&step).expect("json");
        assert_eq!(json, serde_json::json!({ "item": "i1", "step": "develop" }));
        assert_eq!(serde_json::from_value::<Attribution>(serde_json::json!({})).expect("old"), Attribution::default());
    }
}
