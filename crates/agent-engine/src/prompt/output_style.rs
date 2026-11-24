use serde_json::Value;

const FOLDER: &str = "output-styles/";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OutputStyle {
    pub key: String,
    pub name: String,
    pub description: String,
    #[serde(skip)]
    pub body: String,
    #[serde(skip)]
    pub turn_reminder: Option<String>,
}

impl OutputStyle {
    fn parse(key: &str, text: &str) -> Option<Self> {
        let (front, body) = super::markdown::frontmatter(text).ok()?;
        let field = |name: &str| front.get(name).and_then(Value::as_str).map(str::trim).filter(|text| !text.is_empty());
        Some(Self {
            key: key.to_owned(),
            name: field("name").unwrap_or(key).to_owned(),
            description: field("description").unwrap_or_default().to_owned(),
            body: body.trim().to_owned(),
            turn_reminder: field("turn-reminder").map(str::to_owned),
        })
    }

    pub fn system(&self) -> String {
        format!("\n\n# Output Style: {}\n{}", self.name, self.body)
    }
}

pub fn all() -> Vec<OutputStyle> {
    content::builtin_files(FOLDER)
        .into_iter()
        .filter_map(|(path, _)| path.strip_prefix(FOLDER)?.strip_suffix(".md"))
        .filter_map(find)
        .collect()
}

pub fn find(key: &str) -> Option<OutputStyle> {
    let plain = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !plain {
        return None;
    }
    let text = content::source(&format!("{FOLDER}{key}.md"))?;
    OutputStyle::parse(key, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtin_styles_are_listed_with_the_brand_name() {
        let keys: Vec<String> = all().into_iter().map(|style| style.key).collect();
        assert_eq!(keys, ["concise", "explanatory", "learning", "proactive"]);
        let concise = find("concise").expect("concise");
        assert_eq!(concise.name, "Concise");
        assert!(concise.description.starts_with(brand::NAME));
        assert!(concise.system().starts_with("\n\n# Output Style: Concise\n"));
        assert!(concise.turn_reminder.is_some());
        assert!(find("explanatory").expect("explanatory").turn_reminder.is_none());
    }

    #[test]
    fn an_unknown_or_unsafe_key_finds_nothing() {
        assert!(find("nope").is_none());
        assert!(find("../system").is_none());
        assert!(find("").is_none());
    }
}
