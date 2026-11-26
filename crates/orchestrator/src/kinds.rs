use std::path::Path;

use domain::Language;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const FILE: &str = "repository-kinds.ts";
const EXPORT: &str = "kinds";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryKind {
    pub key: String,
    #[serde(default)]
    pub label: String,
}

fn declared(found: Value) -> Vec<RepositoryKind> {
    let listed = found.as_array().map(Vec::as_slice).unwrap_or_default();
    let mut kept: Vec<RepositoryKind> = Vec::new();
    for entry in listed {
        let Ok(mut kind) = serde_json::from_value::<RepositoryKind>(entry.clone()) else { continue };
        kind.key = kind.key.trim().to_owned();
        if kind.key.is_empty() || kept.iter().any(|known| known.key == kind.key) {
            continue;
        }
        if kind.label.trim().is_empty() {
            kind.label = kind.key.clone();
        }
        kept.push(kind);
    }
    kept
}

pub fn listed_in(root: Option<&Path>, language: Language) -> Vec<RepositoryKind> {
    let asked = json!({ "language": language.code() });
    declared(content::call_in(root, FILE, EXPORT, &asked).unwrap_or_default())
}

pub fn listed(language: Language) -> Vec<RepositoryKind> {
    listed_in(content::folder().as_deref(), language)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(kinds: &[RepositoryKind]) -> Vec<&str> {
        kinds.iter().map(|kind| kind.key.as_str()).collect()
    }

    #[test]
    fn the_built_in_content_declares_the_five_kinds_in_the_language_asked() {
        let english = listed_in(None, Language::English);
        assert_eq!(keys(&english), ["application", "library", "configuration", "infrastructure", "tests"]);
        assert_eq!(english[1].label, "Library");
        let portuguese = listed_in(None, Language::Portuguese);
        assert_eq!(portuguese[0].label, "Aplicação");
        assert_eq!(listed_in(None, Language::Spanish)[4].label, "Pruebas");
    }

    #[test]
    fn the_kinds_of_the_user_replace_the_built_in_ones() {
        let folder = tempfile::tempdir().expect("folder");
        let source = r#"export function kinds({ language }) {
            return [{ key: " mobile ", label: language === "en" ? "Mobile app" : "App móvel" }, { key: "docs" }, { key: "docs", label: "Again" }, { label: "No key" }, { key: "" }];
        }"#;
        std::fs::write(folder.path().join(FILE), source).expect("file");
        let found = listed_in(Some(folder.path()), Language::English);
        let mobile = RepositoryKind { key: "mobile".into(), label: "Mobile app".into() };
        let docs = RepositoryKind { key: "docs".into(), label: "docs".into() };
        assert_eq!(found, [mobile, docs]);
        assert_eq!(serde_json::to_value(&found[0]).expect("json"), json!({ "key": "mobile", "label": "Mobile app" }));
    }

    #[test]
    fn a_declaration_that_is_not_a_list_leaves_no_kinds() {
        assert!(declared(json!({ "key": "x" })).is_empty());
        assert!(declared(Value::Null).is_empty());
    }
}
