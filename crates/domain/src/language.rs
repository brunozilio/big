use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "es")]
    Spanish,
    #[default]
    #[serde(rename = "pt-BR")]
    Portuguese,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::English, Self::Spanish, Self::Portuguese];

    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Spanish => "es",
            Self::Portuguese => "pt-BR",
        }
    }

    pub fn named(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|language| language.code() == code)
    }

    pub fn prompt_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Spanish => "Spanish",
            Self::Portuguese => "Brazilian Portuguese",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_language_is_found_by_its_code_and_serializes_as_it() {
        for language in Language::ALL {
            assert_eq!(Language::named(language.code()), Some(language));
            assert_eq!(serde_json::to_value(language).expect("json"), serde_json::json!(language.code()));
        }
        assert_eq!(Language::named("fr"), None);
        assert_eq!(Language::default().prompt_name(), "Brazilian Portuguese");
    }
}
