use serde_json::json;

use super::*;

fn given(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

#[test]
fn reading_the_repository_s_file_again_gives_what_was_built() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(FILE);
    assert_eq!(Configuration::built().with_file(&file), Configuration::built());
    assert!(!Configuration::built().section("connectors").is_empty());
}

#[test]
fn a_file_read_while_running_adds_sections_and_replaces_keys() {
    let text = "[newcomer]\nurl = \"https://example.test\" # where\npages = 3\n\n[connectors]\nmarker = \"other\"\n";
    let read = Configuration::built().with_text(text);
    assert_eq!(read.section("newcomer"), given(json!({ "url": "https://example.test", "pages": "3" })));
    assert_eq!(read.section("connectors")["marker"], "other");
    assert_eq!(read.section("connectors").len(), Configuration::built().section("connectors").len());
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("no-such-config.toml");
    assert_eq!(Configuration::built().with_file(&missing), Configuration::built());
}

#[test]
fn the_schema_types_fills_defaults_and_keeps_the_text_of_plain_fields() {
    let schema = json!({
        "url": { "required": true },
        "pages": { "type": "integer", "default": 10 },
        "ratio": { "type": "number" },
        "on": { "type": "boolean", "default": "false" },
        "tags": { "type": "list", "choices": ["a", "b"] },
        "note": {}
    });
    let read = applied("x", &schema, &given(json!({ "url": "u", "ratio": "0.5", "tags": "a b" })));
    let wanted = json!({ "url": "u", "pages": 10, "ratio": 0.5, "on": false, "tags": ["a", "b"] });
    assert_eq!(read, Ok(given(wanted)));
    let typed = applied("x", &schema, &given(json!({ "url": "u", "pages": "7", "on": true, "tags": ["b"] })));
    assert_eq!(typed.map(|read| (read["pages"].clone(), read["on"].clone())), Ok((json!(7), json!(true))));
}

#[test]
fn a_section_that_breaks_the_schema_says_every_problem_at_once() {
    let schema =
        json!({ "url": { "required": true }, "pages": { "type": "integer" }, "mode": { "choices": ["fast", "slow"] } });
    let refused = applied("x", &schema, &given(json!({ "pages": "many", "mode": "odd", "uri": "u" })));
    let refused = refused.err().unwrap_or_default();
    assert!(refused.starts_with("the [x] section of config.toml"), "{refused}");
    for problem in [
        "`url` is required",
        "`pages` must be a whole number, not \"many\"",
        "`mode` is `odd`, not one of fast, slow",
        "`uri` is not one of the keys the connector reads (mode, pages, url)",
    ] {
        assert!(refused.contains(problem), "{problem} in {refused}");
    }
}

#[test]
fn a_schema_that_does_not_read_is_the_connector_s_fault() {
    let unknown = applied("x", &json!({ "url": { "type": "date" } }), &Map::new()).err().unwrap_or_default();
    assert!(unknown.starts_with("x: the `config` export is not a valid configuration schema"), "{unknown}");
    let typo = applied("x", &json!({ "url": { "requried": true } }), &Map::new()).err().unwrap_or_default();
    assert!(typo.contains("requried"), "{typo}");
    let wrong = applied("x", &json!({ "pages": { "type": "integer", "default": "ten" } }), &Map::new());
    assert!(wrong.err().unwrap_or_default().contains("the default of the `config` schema is wrong"));
}
