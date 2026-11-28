use serde_json::json;

use super::*;

fn engine(rules: serde_json::Value) -> Engine {
    let (engine, problems) = Engine::compiled(Rules::read(&rules).unwrap_or_default());
    assert!(problems.is_empty(), "{problems:?}");
    engine
}

#[test]
fn tokens_are_hidden_and_the_layout_kept() {
    let output =
        "config:\n    API_KEY=abc123\n\tauth: Bearer eyJhbGc.eyJzdWIi.c2lnbmF0\n  key (ghp_0123456789abcdefghij).\n";

    let shown = redact(output, &Known::default());

    assert_eq!(shown, "config:\n    API_KEY=[redacted]\n\tauth: Bearer [redacted]\n  key ([redacted]).\n");
}

#[test]
fn values_from_env_files_are_hidden_wherever_they_appear() {
    let env = "# comment\nexport DATABASE_URL=\"postgres://u:hunter2hunter@db/x\"\nSHORT=abc\nEMPTY=\nNAME=has spaces in it\n";
    let known = Known::from_env_files([env]);

    let shown = redact("connecting to postgres://u:hunter2hunter@db/x ok, SHORT=abc", &known);

    assert_eq!(shown, "connecting to [redacted] ok, SHORT=abc");
    assert_eq!(found("connecting to postgres://u:hunter2hunter@db/x", &known), [KNOWN]);
    assert!(Known::from_env_files(["A=b"]).is_empty(), "short values would hide ordinary words");
}

#[test]
fn a_parse_error_shows_its_bad_token_unless_the_token_is_a_secret() {
    let parse = "SyntaxError: unexpected token: 'Started'\n    at <input>:1:1";
    let named = "Invalid token: \"background\" and token: abc123";
    let leaked = "SyntaxError: unexpected token: 'ghp_0123456789abcdefghij'";

    assert_eq!(redact(parse, &Known::default()), parse);
    assert_eq!(redact(named, &Known::default()), "Invalid token: \"background\" and token: [redacted]");
    assert_eq!(redact(leaked, &Known::default()), "SyntaxError: unexpected token: '[redacted]'");
}

#[test]
fn ordinary_text_is_left_as_it_is() {
    let text = "cargo test --workspace\n  passed: 12, failed: 0\n";
    assert_eq!(redact(text, &Known::default()), text);
    assert!(found(text, &Known::default()).is_empty());
}

#[test]
fn the_shipped_rules_come_from_content_and_hold_what_left_the_code() {
    let rules = shipped();
    assert!(rules.prefixes.len() >= 12 && rules.jwt && rules.labels.contains(&"bearer".to_owned()));
    let engine = current();
    for token in ["sk-0123456789abcdefghij", "xoxb-12345678", "AKIA0123456789ABCDEF", "glpat-0123456789abcdefghij"] {
        assert!(engine.token(token).is_some(), "{token}");
    }
    assert_eq!(engine.token("AKIA0123456789abcdef"), None, "the shape is upper case and digits");
    assert_eq!(engine.token("ghp_short"), None, "too short to be a token");
}

#[test]
fn each_kind_of_rule_is_generic_and_names_what_it_found() {
    let engine = engine(json!({
        "prefixes": [{ "name": "acme", "prefix": "acme_", "min": 10 }],
        "patterns": [{ "name": "order", "regex": "^ORD-[0-9]{6}$" }],
        "keys": ["passphrase"],
        "labels": ["basic"],
        "jwt": true,
        "entropy": { "minLength": 24, "bits": 4.0 }
    }));
    let said =
        "acme_0123456 ORD-123456 passphrase=x basic dXNlcjpwYXNz eyJhbGc.eyJzdWIi.c2lnbmF0 Zx8Qw2Lr9Tk4Vb7Nm3Hs6Pj1";
    let redaction = engine.redact(said, &Known::default());
    assert_eq!(redaction.text, "[redacted] [redacted] passphrase=[redacted] basic [redacted] [redacted] [redacted]");
    assert_eq!(redaction.found, ["acme", "order", ASSIGNMENT, LABEL, JWT, ENTROPY]);
    assert!(redaction.sensitive());
    let plain = engine.redact("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", &Known::default());
    assert!(!plain.sensitive(), "a hex digest or a repeated letter is not random enough: {plain:?}");
}

#[test]
fn a_rule_set_only_adds_and_a_bad_regex_is_told_not_fatal() {
    let mut rules = shipped();
    let more =
        Rules::read(&json!({ "prefixes": [{ "name": "mine", "prefix": "mine_", "min": 8 }], "keys": ["token"] }));
    rules.merge(&more.unwrap_or_default());
    assert_eq!(rules.keys.iter().filter(|key| *key == "token").count(), 1);
    assert!(rules.prefixes.iter().any(|prefix| prefix.name == "mine"));
    let (engine, problems) =
        Engine::compiled(Rules::read(&json!({ "patterns": [{ "name": "broken", "regex": "(" }] })).unwrap_or_default());
    assert_eq!(problems.len(), 1);
    assert_eq!(engine.token("("), None);
    let (layered, problems) = Rules::layered([&json!({ "keys": ["pin"] }), &json!({ "keys": 3 })]);
    assert_eq!((layered.keys, problems.len()), (vec!["pin".to_owned()], 1));
}

#[test]
fn a_declared_source_joins_the_shipped_rules_and_replaces_its_own_earlier_copy() {
    assert_eq!(redact("mine_0123456789", &Known::default()), "mine_0123456789");
    let mine = Rules::read(&json!({ "prefixes": [{ "name": "mine", "prefix": "mine_", "min": 8 }] }));
    assert!(declare("test", mine.unwrap_or_default()).is_empty());
    assert_eq!(redact("mine_0123456789", &Known::default()), REDACTED);
    assert_eq!(redact("ghp_0123456789abcdefghij", &Known::default()), REDACTED, "the shipped ones stay");
    declare("test", Rules::default());
    assert_eq!(redact("mine_0123456789", &Known::default()), "mine_0123456789");
}

#[test]
fn every_string_in_a_value_is_redacted_and_the_shape_kept() {
    let args = json!({ "title": "Deploy", "body": ["use ghp_0123456789abcdefghij", { "note": "password=x" }], "n": 3 });
    let (redacted, found) = current().redact_value(&args, &Known::default());
    assert_eq!(
        redacted,
        json!({ "title": "Deploy", "body": ["use [redacted]", { "note": "password=[redacted]" }], "n": 3 })
    );
    assert_eq!(found, ["github-token", ASSIGNMENT]);
}
