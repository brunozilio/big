use serde_json::Value;

pub const TABLE: &str = "retired.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retired {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub from: String,
    pub to: String,
    pub names: Vec<String>,
    pub becomes: Vec<(String, String)>,
}

fn read(text: &str) -> Value {
    serde_json::from_str::<Value>(text).unwrap_or_else(|_| {
        tracing::warn!(table = TABLE, "the table of retired imports does not read; no import is redirected");
        Value::Null
    })
}

pub fn parse(text: &str) -> Vec<Retired> {
    let value = read(text);
    let listed = value["imports"].as_array().cloned().unwrap_or_default();
    listed
        .iter()
        .filter_map(|entry| {
            let from = entry["from"].as_str().filter(|from| !from.is_empty())?;
            let to = entry["to"].as_str().filter(|to| !to.is_empty())?;
            Some(Retired { from: from.to_owned(), to: to.to_owned() })
        })
        .collect()
}

pub fn parse_moved(text: &str) -> Vec<Moved> {
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
            Some(Moved { from: from.to_owned(), to: to.to_owned(), names, becomes })
        })
        .collect()
}

pub fn table() -> Vec<Retired> {
    crate::source(TABLE).map(|text| parse(&text)).unwrap_or_default()
}

pub fn moved_table() -> Vec<Moved> {
    crate::source(TABLE).map(|text| parse_moved(&text)).unwrap_or_default()
}

pub fn moved(table: &[Moved], found: &str, names: &[String], importer: &str) -> Vec<String> {
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

pub fn resolve(
    table: &[Retired],
    candidates: &[String],
    exists: &dyn Fn(&str) -> bool,
    importer: &str,
) -> Option<String> {
    let found = candidates.iter().find(|candidate| exists(candidate));
    if let Some(found) = found {
        let kept = table.iter().find(|retired| retired.to == *found && exists(&retired.from));
        if let Some(kept) = kept {
            warned(kept, importer, "your file at the retired path is used; move it to the new one");
            return Some(kept.from.clone());
        }
        if let Some(retired) = table.iter().find(|retired| retired.from == *found) {
            warned(retired, importer, "this path is retired; import the new one");
        }
        return Some(found.clone());
    }
    let moved = candidates.iter().find_map(|candidate| table.iter().find(|retired| retired.from == *candidate));
    let moved = moved.filter(|retired| exists(&retired.to))?;
    warned(moved, importer, "this import is retired and was redirected; import the new path");
    Some(moved.to.clone())
}

fn warned(retired: &Retired, importer: &str, what: &str) {
    tracing::warn!(from = %retired.from, to = %retired.to, importer, "{what}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Vec<Retired> {
        parse(r#"{ "schema_version": 1, "imports": [{ "from": "old/a.ts", "to": "new/a.ts" }] }"#)
    }

    fn candidates(base: &str) -> Vec<String> {
        vec![base.to_owned(), format!("{base}.ts")]
    }

    #[test]
    fn an_import_of_a_retired_path_is_redirected_to_the_new_one() {
        let exists = |path: &str| path == "new/a.ts";
        assert_eq!(resolve(&table(), &candidates("old/a"), &exists, "w.ts").as_deref(), Some("new/a.ts"));
        assert_eq!(resolve(&table(), &candidates("new/a"), &exists, "w.ts").as_deref(), Some("new/a.ts"));
        assert_eq!(resolve(&table(), &candidates("gone/a"), &exists, "w.ts"), None);
    }

    #[test]
    fn a_file_the_user_kept_at_the_retired_path_still_wins() {
        let exists = |path: &str| path == "new/a.ts" || path == "old/a.ts";
        assert_eq!(resolve(&table(), &candidates("new/a"), &exists, "w.ts").as_deref(), Some("old/a.ts"));
        assert_eq!(resolve(&table(), &candidates("old/a"), &exists, "w.ts").as_deref(), Some("old/a.ts"));
    }

    #[test]
    fn a_name_that_moved_warns_only_whoever_imports_it() {
        let table = parse_moved(
            r#"{ "exports": [{ "from": "kit/index.js", "to": "pkg/index.js", "names": ["plan", "merge"], "becomes": { "merge": "kit/base.js" } }] }"#,
        );
        assert_eq!(table[0].becomes, [("merge".to_owned(), "kit/base.js".to_owned())]);
        let names = |list: &[&str]| list.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
        assert_eq!(moved(&table, "kit/index.js", &names(&["draft", "plan"]), "w.ts"), names(&["plan"]));
        assert!(moved(&table, "kit/index.js", &names(&["draft"]), "w.ts").is_empty());
        assert!(moved(&table, "other.js", &names(&["plan"]), "w.ts").is_empty());
    }

    #[test]
    fn the_shipped_names_that_moved_are_exported_by_both_modules() {
        let shipped = super::moved_table();
        assert!(!shipped.is_empty());
        for entry in shipped {
            let old = crate::builtin_text(&entry.from).unwrap_or_else(|| panic!("{} is shipped", entry.from));
            let new = crate::builtin_text(&entry.to).unwrap_or_else(|| panic!("{} is shipped", entry.to));
            for name in &entry.names {
                assert!(old.contains(name.as_str()), "{} still offers {name} under the old alias", entry.from);
                assert!(new.contains(name.as_str()), "{} exports {name}", entry.to);
            }
            for (name, module) in &entry.becomes {
                let later = crate::builtin_text(module).unwrap_or_else(|| panic!("{module} is shipped"));
                assert!(entry.names.contains(name) && later.contains(&format!("as {name} ")), "{module} offers {name}");
            }
        }
    }

    #[test]
    fn the_shipped_table_redirects_only_to_paths_that_exist() {
        let shipped = super::table();
        assert!(!shipped.is_empty());
        for retired in shipped {
            assert!(crate::shipped(&retired.to).is_some(), "{} points to a missing {}", retired.from, retired.to);
            assert!(crate::shipped(&retired.from).is_none(), "{} is retired but still shipped", retired.from);
        }
    }
}
