use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub export: String,
    pub matcher: Option<Value>,
    pub timeout: Option<Value>,
}

fn is_function(value: &Value) -> bool {
    value["$function"] == true
}

pub fn entries(outline: &Value, export: &str) -> Result<Vec<Entry>, String> {
    let listed: Vec<(String, &Value)> = match outline.as_array() {
        Some(items) => items.iter().enumerate().map(|(at, item)| (format!("{export}.{at}"), item)).collect(),
        None => vec![(export.to_owned(), outline)],
    };
    let mut found = Vec::new();
    for (export, entry) in listed {
        if is_function(entry) {
            found.push(Entry { export, matcher: None, timeout: None });
        } else if is_function(&entry["run"]) {
            found.push(Entry {
                export: format!("{export}.run"),
                matcher: entry.get("matcher").cloned(),
                timeout: entry.get("timeout").cloned(),
            });
        } else {
            return Err(format!("{export}: each entry is a function or an object with a `run` function"));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_function_an_object_with_run_and_a_list_of_them_are_all_entries() {
        let function = json!({ "$function": true });
        let single = entries(&function, "intercept.model").expect("single");
        assert_eq!(single, [Entry { export: "intercept.model".into(), matcher: None, timeout: None }]);
        let listed = entries(&json!([function, { "matcher": "Bash", "timeout": 5, "run": function }]), "hooks.Stop")
            .expect("listed");
        assert_eq!(listed[0].export, "hooks.Stop.0");
        assert_eq!(listed[1].export, "hooks.Stop.1.run");
        assert_eq!((listed[1].matcher.clone(), listed[1].timeout.clone()), (Some(json!("Bash")), Some(json!(5))));
        let refused = entries(&json!([{ "matcher": "x" }]), "hooks.Stop").expect_err("refused");
        assert!(refused.starts_with("hooks.Stop.0: each entry"), "{refused}");
    }
}
