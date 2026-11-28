use std::io::Write;
use std::sync::{Arc, Mutex};

use domain::redact::Rules;
use serde_json::{Value, json};

use super::*;

#[derive(Clone, Default)]
struct Kept(Arc<Mutex<Vec<u8>>>);

impl Write for Kept {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.locked().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Kept {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.locked()).into_owned()
    }
}

fn redactor() -> Arc<Redactor> {
    let rules = json!({ "labels": ["bearer"], "keys": ["token"], "valueMin": 6, "labelMin": 3 });
    Arc::new(Redactor::compiled(Rules::read(&rules).unwrap_or_default()).0)
}

fn outbound(shape: Shape, sharing: Sharing) -> (Outbound, Kept) {
    let kept = Kept::default();
    let outbound = Outbound::new(shape, sharing, Keys::default(), redactor(), Box::new(kept.clone()));
    (outbound, kept)
}

fn written(outbound: &Outbound, line: &str) {
    let mut writer = outbound.line();
    let _ = writer.write_all(line.as_bytes());
}

#[test]
fn a_connector_log_leaves_redacted_and_names_the_policy() {
    let (outbound, kept) = outbound(Shape::Json, Sharing::Full);
    written(&outbound, "{\"level\":\"INFO\",\"message\":\"Bearer abc\",\"target\":\"connector\"}\n");
    let line: Value = serde_json::from_str(kept.text().trim_end()).unwrap_or_default();
    assert_eq!(line["message"], "Bearer [redacted]");
    assert_eq!(line["target"], "connector");
    assert_eq!(line["policy_revision"].as_str(), Some(outbound.revision().as_str()));
    assert!(!kept.text().contains("abc"));
}

#[test]
fn a_secret_field_is_redacted_whatever_its_value_looks_like() {
    let (outbound, kept) = outbound(Shape::Json, Sharing::Full);
    written(&outbound, "{\"message\":\"signed in\",\"session.token\":\"short\",\"count\":3}\n");
    let line: Value = serde_json::from_str(kept.text().trim_end()).unwrap_or_default();
    assert_eq!((line["session.token"].as_str(), line["count"].as_u64()), (Some("[redacted]"), Some(3)));
    assert_eq!(line["message"], "signed in");
}

#[test]
fn a_record_the_redactor_cannot_read_is_dropped_and_counted() {
    let (outbound, kept) = outbound(Shape::Json, Sharing::Full);
    let before = capture::dropped();
    written(&outbound, "not json Bearer abc\n");
    assert_eq!(kept.text(), "");
    assert!(capture::dropped() > before);
}

#[test]
fn a_metadata_sink_keeps_no_content_and_a_disabled_one_keeps_nothing() {
    let (metadata, kept) = outbound(Shape::Json, Sharing::Metadata);
    written(&metadata, "{\"message\":\"tool.start\",\"gen_ai.tool.call.arguments\":\"{}\"}\n");
    assert!(kept.text().contains("tool.start") && !kept.text().contains("arguments"), "{}", kept.text());
    let (disabled, nothing) = outbound(Shape::Json, Sharing::Disabled);
    written(&disabled, "{\"message\":\"tool.start\"}\n");
    assert_eq!(nothing.text(), "");
}

#[test]
fn a_text_line_is_redacted_as_text() {
    let (outbound, kept) = outbound(Shape::Text, Sharing::Full);
    written(&outbound, "INFO connector: Bearer abc\n");
    assert_eq!(kept.text(), "INFO connector: Bearer [redacted]\n");
}

#[test]
fn a_wider_redactor_attached_later_changes_the_revision() {
    let (outbound, _) = outbound(Shape::Json, Sharing::Full);
    let before = outbound.revision();
    let rules = json!({ "labels": ["bearer", "basic"], "valueMin": 6 });
    outbound.redacting(Arc::new(Redactor::compiled(Rules::read(&rules).unwrap_or_default()).0));
    assert_ne!(outbound.revision(), before);
}

#[test]
fn the_fields_keep_the_order_they_were_written_in() {
    let (outbound, kept) = outbound(Shape::Json, Sharing::Full);
    written(&outbound, "{\"timestamp\":\"t\",\"level\":\"INFO\",\"message\":\"m\",\"target\":\"x\"}\n");
    let expected = format!(
        "{{\"timestamp\":\"t\",\"level\":\"INFO\",\"message\":\"m\",\"target\":\"x\",\"policy_revision\":\"{}\"}}\n",
        outbound.revision()
    );
    assert_eq!(kept.text(), expected);
}

struct Panicking {
    rules: Rules,
    outbound: std::sync::OnceLock<std::sync::Weak<Outbound>>,
}

impl Panicking {
    fn told_and_failed(&self) -> ! {
        if let Some(outbound) = self.outbound.get().and_then(std::sync::Weak::upgrade) {
            written(&outbound, "{\"level\":\"ERROR\",\"message\":\"the redactor panicked\"}\n");
        }
        panic!("the redactor broke");
    }
}

impl Redacts for Panicking {
    fn rules(&self) -> &Rules {
        &self.rules
    }

    fn text(&self, _: &str) -> String {
        self.told_and_failed()
    }

    fn value(&self, _: &str, _: &Value) -> Value {
        self.told_and_failed()
    }
}

#[test]
fn a_redactor_that_panics_and_logs_again_drops_both_lines_without_redacting_twice() {
    let kept = Kept::default();
    let panicking = Arc::new(Panicking { rules: Rules::default(), outbound: std::sync::OnceLock::new() });
    let outbound = Arc::new(Outbound::new(
        Shape::Json,
        Sharing::Full,
        Keys::default(),
        Arc::clone(&panicking) as Arc<dyn Redacts>,
        Box::new(kept.clone()),
    ));
    let _ = panicking.outbound.set(Arc::downgrade(&outbound));
    let before = capture::dropped();
    written(&outbound, "{\"level\":\"INFO\",\"message\":\"Bearer abc\"}\n");
    assert_eq!(kept.text(), "");
    assert!(capture::dropped() >= before + 2, "the line and the one logged while redacting are both counted");
    outbound.redacting(redactor());
    written(&outbound, "{\"message\":\"Bearer abc\"}\n");
    assert!(kept.text().contains("Bearer [redacted]"), "the guard is released after the panic: {}", kept.text());
}
