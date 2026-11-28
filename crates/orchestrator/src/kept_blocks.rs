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
    let named = raw["type"].as_str().is_some_and(|kind| CHANGE_BLOCKS.contains(&kind));
    if !named || !raw["change"].is_null() || raw[RETIRED_REF].is_null() {
        return raw.clone();
    }
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

    #[test]
    fn a_change_block_kept_before_the_opaque_reference_reads_as_the_new_one() {
        let kept = json!([
            { "type": "merge", "pr": "o/r#3", "methods": ["squash"] },
            { "type": "review", "change": "o/r#4", "capability": "forge" },
            { "type": "markdown", "text": "x", "pr": "y" },
        ]);
        let read = upgraded(&kept);
        assert_eq!(read[0]["change"], "o/r#3");
        assert_eq!(read[0]["capability"], connectors::contracts::CODE);
        assert_eq!(read[1], kept[1]);
        assert_eq!(read[2], kept[2]);
        assert_eq!(upgraded(&kept[0])["change"], "o/r#3");
    }
}
