use serde_json::Value;

pub const TABLE: &str = "retired.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retired {
    pub from: String,
    pub to: String,
}

pub(crate) fn read(text: &str) -> Value {
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

pub fn table() -> Vec<Retired> {
    crate::source(TABLE).map(|text| parse(&text)).unwrap_or_default()
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
    fn the_shipped_table_redirects_only_to_paths_that_exist() {
        let shipped = super::table();
        assert!(!shipped.is_empty());
        for retired in shipped {
            assert!(crate::shipped(&retired.to).is_some(), "{} points to a missing {}", retired.from, retired.to);
            assert!(crate::shipped(&retired.from).is_none(), "{} is retired but still shipped", retired.from);
        }
    }
}
