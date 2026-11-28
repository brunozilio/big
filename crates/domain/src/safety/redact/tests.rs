use serde_json::json;

use super::*;

const TOKEN: &str = "house_0123456789abcdef";

fn redactor(rules: serde_json::Value) -> Redactor {
    let (redactor, problems) = Redactor::compiled(Rules::read(&rules).unwrap_or_default());
    assert!(problems.is_empty(), "{problems:?}");
    redactor
}

fn redact(text: &str, known: &Known) -> String {
    testing().redact(text, known).text
}

#[test]
fn tokens_are_hidden_and_the_layout_kept() {
    let output = format!("config:\n    API_KEY=abc123\n\tauth: Bearer eyJhbGc.eyJzdWIi.c2lnbmF0\n  key ({TOKEN}).\n");

    let shown = redact(&output, &Known::default());

    assert_eq!(shown, "config:\n    API_KEY=[redacted]\n\tauth: Bearer [redacted]\n  key ([redacted]).\n");
}

#[test]
fn values_from_env_files_are_hidden_wherever_they_appear() {
    let env = "# comment\nexport DATABASE_URL=\"postgres://u:hunter2hunter@db/x\"\nSHORT=abc\nEMPTY=\nNAME=has spaces in it\n";
    let known = Known::from_env_files([env]);

    let shown = testing().redact("connecting to postgres://u:hunter2hunter@db/x ok, SHORT=abc", &known);

    assert_eq!(shown.text, "connecting to [redacted] ok, SHORT=abc");
    assert_eq!(shown.found, [KNOWN]);
    assert!(Known::from_env_files(["A=b"]).is_empty(), "short values would hide ordinary words");
}

#[test]
fn a_parse_error_shows_its_bad_token_unless_the_token_is_a_secret() {
    let parse = "SyntaxError: unexpected token: 'Started'\n    at <input>:1:1";
    let named = "Invalid token: \"background\" and token: abc123";
    let leaked = format!("SyntaxError: unexpected token: '{TOKEN}'");

    assert_eq!(redact(parse, &Known::default()), parse);
    assert_eq!(redact(named, &Known::default()), "Invalid token: \"background\" and token: [redacted]");
    assert_eq!(redact(&leaked, &Known::default()), "SyntaxError: unexpected token: '[redacted]'");
}

#[test]
fn ordinary_text_is_left_as_it_is() {
    let text = "cargo test --workspace\n  passed: 12, failed: 0\n";
    let redaction = testing().redact(text, &Known::default());
    assert_eq!(redaction.text, text);
    assert!(!redaction.sensitive());
}

#[test]
fn a_key_or_a_label_is_redacted_only_when_its_value_looks_like_a_secret() {
    for text in ["token: string", "password: String", "the Bearer of bad news", "password=x", "token: number,"] {
        assert_eq!(redact(text, &Known::default()), text);
    }
    assert_eq!(redact("password: hunter22", &Known::default()), "password: [redacted]");
    assert_eq!(redact("Bearer abc.def-123", &Known::default()), "Bearer [redacted]");
}

#[test]
fn a_label_may_ask_for_a_shorter_value_than_a_bare_word() {
    let shorter = redactor(json!({ "labels": ["bearer"], "valueMin": 6, "labelMin": 3 }));
    assert_eq!(shorter.redact("Bearer abc", &Known::default()).text, "Bearer [redacted]");
    assert_eq!(shorter.redact("the Bearer of bad news", &Known::default()).text, "the Bearer of bad news");
    let unset = redactor(json!({ "labels": ["bearer"], "valueMin": 6 }));
    assert_eq!(unset.redact("Bearer abc", &Known::default()).text, "Bearer abc");
}

#[test]
fn each_kind_of_rule_is_generic_and_names_what_it_found() {
    let redactor = redactor(json!({
        "prefixes": [{ "name": "acme", "prefix": "acme_", "min": 10 }],
        "patterns": [{ "name": "order", "regex": "^ORD-[0-9]{6}$" }],
        "keys": ["passphrase"],
        "labels": ["basic"],
        "jwt": true,
        "entropy": { "minLength": 24, "bits": 4.0 }
    }));
    let said =
        "acme_0123456 ORD-123456 passphrase=x basic dXNlcjpwYXNz eyJhbGc.eyJzdWIi.c2lnbmF0 Zx8Qw2Lr9Tk4Vb7Nm3Hs6Pj1";
    let redaction = redactor.redact(said, &Known::default());
    assert_eq!(redaction.text, "[redacted] [redacted] passphrase=[redacted] basic [redacted] [redacted] [redacted]");
    assert_eq!(redaction.found, ["acme", "order", ASSIGNMENT, LABEL, JWT, ENTROPY]);
    assert!(redaction.sensitive());
    let plain = redactor.redact("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", &Known::default());
    assert!(!plain.sensitive(), "a hex digest or a repeated letter is not random enough: {plain:?}");
}

#[test]
fn a_rule_set_only_adds_and_a_bad_regex_is_told_not_fatal() {
    let mut rules = testing().rules().clone();
    let more =
        Rules::read(&json!({ "prefixes": [{ "name": "mine", "prefix": "mine_", "min": 8 }], "keys": ["token"] }));
    rules.merge(&more.unwrap_or_default());
    assert_eq!(rules.keys.iter().filter(|key| *key == "token").count(), 1);
    assert!(rules.prefixes.iter().any(|prefix| prefix.name == "mine"));
    let (redactor, problems) = Redactor::compiled(
        Rules::read(&json!({ "patterns": [{ "name": "broken", "regex": "(" }] })).unwrap_or_default(),
    );
    assert_eq!(problems.len(), 1);
    assert_eq!(redactor.token("("), None);
}

#[test]
fn a_connector_declares_only_the_shape_of_its_tokens() {
    let declared = Rules::read(&json!({
        "prefixes": [{ "name": "echo", "prefix": "echo_", "min": 8 }],
        "patterns": [{ "name": "ticket", "regex": "^TCK[0-9]{8}$" }],
        "keys": ["name"],
        "labels": ["the"],
        "valueMin": 1,
        "jwt": true,
        "entropy": { "minLength": 4, "bits": 0.5 }
    }));
    let shapes = declared.unwrap_or_default().shapes();
    assert_eq!((shapes.prefixes.len(), shapes.patterns.len()), (1, 1));
    assert!(shapes.keys.is_empty() && shapes.labels.is_empty() && !shapes.jwt);
    assert_eq!((shapes.entropy, shapes.value_min), (None, None));
}

#[test]
fn every_string_in_a_value_is_redacted_and_the_shape_kept() {
    let args = json!({ "title": "Deploy", "body": [format!("use {TOKEN}"), { "note": "password=hunter22" }], "n": 3 });
    let (redacted, found) = testing().redact_value(&args, &Known::default());
    assert_eq!(
        redacted,
        json!({ "title": "Deploy", "body": ["use [redacted]", { "note": "password=[redacted]" }], "n": 3 })
    );
    assert_eq!(found, ["house", ASSIGNMENT]);
}

const SHIPPED: &str = r#"{ "schema_version": 1, "prefixes": [{ "name": "house", "prefix": "house_", "min": 10 }],
    "keys": ["password"], "valueMin": 6, "jwt": true }"#;

#[test]
fn the_user_s_copy_adds_and_sets_thresholds_while_a_connector_only_adds_shapes() {
    let mine = r#"{ "schema_version": 1, "prefixes": [{ "name": "mine", "prefix": "mine_", "min": 10 }],
        "entropy": { "minLength": 30, "bits": 4.5 } }"#;
    let declared = Rules::read(&json!({
        "prefixes": [{ "name": "echo", "prefix": "echo_", "min": 10 }],
        "entropy": { "minLength": 4, "bits": 0.5 },
        "valueMin": 1,
        "keys": ["name"]
    }))
    .unwrap_or_default();
    let (redactor, problems) = assemble(Some(SHIPPED), Some(mine), &[declared]);
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(redactor.rules().entropy.map(|entropy| entropy.min_length), Some(30));
    assert_eq!(redactor.rules().value_min, Some(6));
    let said = redactor.redact("house_0123456789 mine_0123456789 echo_0123456789 name=ab", &Known::default());
    assert_eq!(said.text, "[redacted] [redacted] [redacted] name=ab");
}

#[test]
fn broken_shipped_rules_are_an_error_and_a_broken_copy_only_a_warning() {
    let (_, problems) = assemble(Some("{ not json"), None, &[]);
    assert_eq!(problems.iter().map(|problem| problem.severity).collect::<Vec<_>>(), [Severity::Error]);
    let (_, missing) = shipped(None);
    assert!(missing.is_some_and(|problem| problem.severity == Severity::Error));
    let (redactor, problems) = assemble(Some(SHIPPED), Some("{ not json"), &[]);
    assert_eq!(problems.iter().map(|problem| problem.severity).collect::<Vec<_>>(), [Severity::Warning]);
    assert_eq!(redactor.rules().prefixes.len(), 1, "the shipped rules stay");
    let newer = layer(r#"{ "schema_version": 9 }"#);
    assert!(newer.is_err());
}
