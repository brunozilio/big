use serde_json::{Value, json};

const CHANGE_BLOCKS: [&str; 3] = ["change", "review", "merge"];
const RETIRED_REF: &str = "pr";

pub fn upgraded(view: &Value) -> Value {
    match view {
        Value::Array(blocks) => Value::Array(blocks.iter().map(block).collect()),
        Value::Object(_) => block(view),
        other => other.clone(),
    }
}

fn block(raw: &Value) -> Value {
    let kind = raw["type"].as_str().filter(|kind| CHANGE_BLOCKS.contains(kind));
    let Some(kind) = kind.filter(|_| raw["change"].is_null() && !raw[RETIRED_REF].is_null()) else {
        return raw.clone();
    };
    tracing::warn!(
        block = kind,
        "a {kind} block that names its change as {RETIRED_REF} retires soon: send change and capability instead \
         (the sdk change, review and merge helpers send both); it is read as change with the {} capability",
        connectors::contracts::CODE
    );
    let mut block = raw.clone();
    block["change"] = raw[RETIRED_REF].clone();
    if block["capability"].is_null() {
        block["capability"] = json!(connectors::contracts::CODE);
    }
    block
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::retired::Heard;

    #[test]
    fn a_change_block_that_still_names_pr_reads_as_the_new_one_and_is_warned() {
        let given = json!([
            { "type": "merge", "pr": "o/r#3", "methods": ["squash"] },
            { "type": "review", "pr": "o/r#5", "capability": "forge" },
            { "type": "review", "change": "o/r#4", "capability": "forge" },
            { "type": "markdown", "text": "x", "pr": "y" },
        ]);
        let heard = Heard::default();
        let read = tracing::subscriber::with_default(heard.clone(), || upgraded(&given));
        assert_eq!(read[0]["change"], "o/r#3");
        assert_eq!(read[0]["capability"], connectors::contracts::CODE);
        assert_eq!(read[0]["methods"], json!(["squash"]));
        assert_eq!((read[1]["change"].clone(), read[1]["capability"].clone()), (json!("o/r#5"), json!("forge")));
        assert_eq!(read[2], given[2]);
        assert_eq!(read[3], given[3]);
        assert_eq!(upgraded(&given[0])["change"], "o/r#3");
        let lines = heard.0.lock().expect("lock").clone();
        let warned: Vec<&String> = lines.iter().filter(|line| line.contains("retires soon")).collect();
        assert_eq!(warned.len(), 2, "{lines:?}");
        assert!(warned[0].contains("block=\"merge\""), "{lines:?}");
    }
}
