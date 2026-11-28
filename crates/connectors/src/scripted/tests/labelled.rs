use domain::label::Trust;
use serde_json::json;

use super::{installed, loaded};
use crate::descriptor::ActionSpec;
use crate::provider::Provider;

const LABELLED: &str = r#"import { alters, looks, object, read, write } from "sdk/connectors/contracts.ts";
export function provider() {
  return { schema_version: 2, name: "Labelled", capabilities: ["notes"], auth: { kind: "token", fields: [] },
    actions: [
      read("notes.get", object([]), object([]), { ...looks(true, true), outputLevel: "internal" }),
      read("notes.mine", object([]), object([]), { ...looks(), trust: "user-authored" }),
      write("notes.send", object([]), object([]), "native", alters("high", false, { effects: ["send-external"], maxInputLevel: "internal" })),
    ] };
}
export function labels({ action, result }) { return action === "notes.get" && result.secret ? "confidential" : null; }
export function call() { return null; }
"#;

#[test]
fn a_connector_declares_trust_levels_and_a_label_read_from_the_result() {
    let (_folder, labelled) = installed("labelled", LABELLED);
    let labelled = labelled.unwrap_or_else(|error| panic!("{error}"));
    let descriptor = labelled.descriptor();
    let get = descriptor.action("notes.get").expect("notes.get");
    assert_eq!((get.trusted(), get.output_level.as_deref()), (Trust::External, Some("internal")));
    assert_eq!(descriptor.action("notes.mine").map(ActionSpec::trusted), Some(Trust::UserAuthored));
    let send = descriptor.action("notes.send").expect("notes.send");
    assert_eq!(send.max_input_level.as_deref(), Some("internal"));
    assert!(send.risky() && !get.risky());
    assert_eq!(labelled.labelled("notes.get", &json!({ "secret": true })).as_deref(), Some("confidential"));
    assert_eq!(labelled.labelled("notes.get", &json!({ "secret": false })), None);
    assert_eq!(loaded("quick").labelled("notes.get", &json!({})), None, "no labels export, no label");
}
