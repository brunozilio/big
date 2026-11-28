use serde_json::Value;

use crate::retired::{TABLE, read};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub from: String,
    pub to: String,
    pub names: Vec<String>,
    pub becomes: Vec<(String, String)>,
    pub alias: Option<String>,
}

pub fn parse(text: &str) -> Vec<Moved> {
    let listed = read(text)["exports"].as_array().cloned().unwrap_or_default();
    listed
        .iter()
        .filter_map(|entry| {
            let from = entry["from"].as_str().filter(|from| !from.is_empty())?;
            let to = entry["to"].as_str().filter(|to| !to.is_empty())?;
            let names = entry["names"].as_array()?.iter().filter_map(Value::as_str).map(str::to_owned).collect();
            let becomes = entry["becomes"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(name, module)| Some((name.clone(), module.as_str()?.to_owned())))
                .collect();
            let alias = entry["alias"].as_str().filter(|alias| !alias.is_empty()).map(str::to_owned);
            Some(Moved { from: from.to_owned(), to: to.to_owned(), names, becomes, alias })
        })
        .collect()
}

pub fn table() -> Vec<Moved> {
    crate::source(TABLE).map(|text| parse(&text)).unwrap_or_default()
}

pub fn warned(table: &[Moved], found: &str, names: &[String], importer: &str) -> Vec<String> {
    let Some(entry) = table.iter().find(|entry| entry.from == found) else { return Vec::new() };
    let taken: Vec<String> = names.iter().filter(|name| entry.names.contains(name)).cloned().collect();
    if !taken.is_empty() {
        let names = taken.join(", ");
        tracing::warn!(from = %entry.from, to = %entry.to, importer, names, "these names leave this module soon; import them from the new one");
    }
    for (name, module) in entry.becomes.iter().filter(|(name, _)| taken.contains(name)) {
        tracing::warn!(from = %entry.from, module = %module, importer, name = %name, "once retired, this name here means the one of the other module");
    }
    taken
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_moved_warns_only_whoever_imports_it() {
        let table = parse(
            r#"{ "exports": [{ "from": "kit/index.js", "to": "pkg/index.js", "names": ["plan", "merge"], "becomes": { "merge": "kit/base.js" }, "alias": "kit/legacy.js" }] }"#,
        );
        assert_eq!(table[0].becomes, [("merge".to_owned(), "kit/base.js".to_owned())]);
        assert_eq!(table[0].alias.as_deref(), Some("kit/legacy.js"));
        let names = |list: &[&str]| list.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
        assert_eq!(warned(&table, "kit/index.js", &names(&["draft", "plan"]), "w.ts"), names(&["plan"]));
        assert!(warned(&table, "kit/index.js", &names(&["draft"]), "w.ts").is_empty());
        assert!(warned(&table, "other.js", &names(&["plan"]), "w.ts").is_empty());
    }

    #[test]
    fn the_shipped_names_that_moved_are_exported_by_both_modules() {
        let shipped = table();
        assert!(!shipped.is_empty());
        for entry in shipped {
            let old = crate::builtin_text(&entry.from).unwrap_or_else(|| panic!("{} is shipped", entry.from));
            let new = crate::builtin_text(&entry.to).unwrap_or_else(|| panic!("{} is shipped", entry.to));
            let alias = entry.alias.as_deref().and_then(crate::builtin_text).unwrap_or_default();
            for name in &entry.names {
                assert!(
                    old.contains(name.as_str()) && alias.contains(name.as_str()),
                    "{} still offers {name}",
                    entry.from
                );
                assert!(new.contains(name.as_str()), "{} exports {name}", entry.to);
            }
            for (name, module) in &entry.becomes {
                let later = crate::builtin_text(module).unwrap_or_else(|| panic!("{module} is shipped"));
                assert!(entry.names.contains(name) && later.contains(&format!("as {name} ")), "{module} offers {name}");
            }
        }
    }

    #[test]
    fn the_core_of_the_sdk_reaches_a_package_only_through_the_alias_marked_to_retire() {
        let aliases: Vec<String> = table().into_iter().filter_map(|entry| entry.alias).collect();
        assert_eq!(aliases, ["sdk/steps/legacy.js"]);
        for (path, _) in crate::builtin_files("sdk/") {
            let text = crate::builtin_text(path).unwrap_or_default();
            let reaches = text.contains("packages/");
            assert!(!reaches || aliases.iter().any(|alias| alias == path), "{path} depends on a package");
        }
    }
}
