use domain::label::{Levels, Mark, Mode, Read, Sink, Trust};
use domain::permission::{Reason, Surface, Verdict};
use serde_json::json;

use super::*;
use crate::policy_gate::Asked;

const ASKED: Asked<'static> = Asked { surface: Surface::Workflow, stage: "send", owner: "local", operation: None };

fn enforced() -> Levels {
    levels_of("export const labels = { mode: \"enforce\", levels: [\"public\", \"internal\", \"confidential\"] };")
}

fn confidential() -> Mark {
    let read = Read::new("Vault record", Trust::System, Some("confidential".into()));
    Mark::of(&enforced(), &[read])
}

#[test]
fn the_shipped_levels_stay_in_shadow() {
    let shipped = levels_of(&content::builtin_text(FILE).expect("shipped"));
    assert_eq!(shipped.mode, Mode::Shadow);
    assert!(!shipped.enforced());
    assert!(shipped.rank("confidential").is_some());
}

#[test]
fn a_broken_or_missing_levels_file_stays_in_shadow() {
    assert_eq!(levels_of("export const labels = {;"), Levels::default());
    assert_eq!(levels_of("export const other = 1;"), Levels::default());
    assert_eq!(levels_of("export const labels = { levels: [\"a\", \"a\"], mode: \"enforce\" };"), Levels::default());
}

#[test]
fn a_crossing_asks_with_its_reason_only_when_enforced() {
    let sink = Sink { ceiling: Some("internal"), risky: false };
    let Verdict::Ask { reason: Reason::Flow(why), rememberable: false } =
        flow(ASKED, "x_send", &enforced(), &confidential(), &sink)
    else {
        panic!("enforced levels ask");
    };
    assert!(why.contains("confidential") && why.contains("internal"), "{why}");
    let shadow = Levels { mode: Mode::Shadow, ..enforced() };
    assert_eq!(flow(ASKED, "x_send", &shadow, &confidential(), &sink), Verdict::Allow, "shadow only logs");
    let open = Sink { ceiling: Some("confidential"), risky: false };
    assert_eq!(flow(ASKED, "x_send", &enforced(), &confidential(), &open), Verdict::Allow);
}

fn classifier(mode: &str) -> Classifier {
    let source = format!(
        "export const mode = \"{mode}\";\nexport function classify(read) {{ return String(read.result.body).includes(\"secret\") ? {{ label: \"confidential\", trust: \"external\" }} : null; }}"
    );
    Classifier { path: "safety/classifiers/words.ts".into(), source }
}

#[test]
fn a_classifier_raises_the_label_when_enforced_and_only_logs_in_shadow() {
    let read = Read::new("Notes", Trust::System, Some("internal".into()));
    let value = json!({ "body": "a secret plan" });
    let raised = classified_by(&[classifier("enforce")], &enforced(), read.clone(), &value);
    assert_eq!((raised.label.as_deref(), raised.trust), (Some("confidential"), Trust::External));
    assert_eq!(classified_by(&[classifier("shadow")], &enforced(), read.clone(), &value), read);
    let plain = json!({ "body": "lunch" });
    assert_eq!(classified_by(&[classifier("enforce")], &enforced(), read.clone(), &plain), read);
    let lower = Read::new("Notes", Trust::System, Some("confidential".into()));
    let quiet =
        Classifier { source: classifier("enforce").source.replace("confidential", "public"), ..classifier("enforce") };
    assert_eq!(classified_by(&[quiet], &enforced(), lower.clone(), &value).label, lower.label, "never lowers");
}
