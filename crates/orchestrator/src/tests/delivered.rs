use domain::workflow::{Audience, Ending};
use serde_json::json;

use super::*;

const OPINION: &str = r#"
import { workflow, step } from "sdk";
const write = step({ name: "write", async run(ctx) { return ctx.pass({ report: "parecer.md", url: "/api/artifacts/a1/content" }); } });
export default workflow({ name: "opinion", output: { kind: "report", audience: "group:legal" }, steps: [write()] });
"#;

#[tokio::test(flavor = "multi_thread")]
async fn a_workflow_that_declares_a_report_ends_with_the_report_its_step_wrote() {
    let world = World::new();
    world.workflow("opinion", OPINION);
    let kernel = world.start().await;
    let id = kernel.create_item("opinion", json!({ "title": "Contrato" })).expect("item");
    let item = ended(&kernel, &id).await;
    let Some(Ending::Finished { delivered: Some(delivered), .. }) = item.ending else { panic!("{:?}", item.ending) };
    assert_eq!((delivered.kind.as_str(), delivered.key.as_str()), ("report", "parecer.md"));
    assert_eq!(delivered.url.as_deref(), Some("/api/artifacts/a1/content"));
    assert_eq!(delivered.audience, Audience::Group("legal".into()));
    assert_eq!(delivered.hash.len(), 64);
    assert!(delivered.at > 0 && delivered.done);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_workflow_without_an_output_ends_with_nothing_delivered() {
    let world = World::new();
    world.workflow("plain", &OPINION.replace("output: { kind: \"report\", audience: \"group:legal\" }, ", ""));
    let kernel = world.start().await;
    let id = kernel.create_item("plain", json!({})).expect("item");
    let item = ended(&kernel, &id).await;
    assert!(matches!(item.ending, Some(Ending::Finished { delivered: None, .. })));
}
