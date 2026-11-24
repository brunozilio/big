use std::path::Path;
use std::sync::Arc;

use serde_json::json;

use crate::model::http::ScriptedHttp;
use crate::tools::script_tool::parse_source;
use crate::tools::{Network, Tool, Workspace};

const STREAMING: &str = r#"
export const spec = { name: "Streamer", input: { type: "object" } };

export function execute(input: { url: string }, ctx: any): string {
  const signed = ctx.crypto.hmac({ key: "k", data: "d", encoding: "base64" });
  const open = ctx.http.stream.open({ url: input.url, body: { signed }, framing: "sse" });
  const seen: string[] = [];
  for (let event = ctx.http.stream.next(open.id); event !== null; event = ctx.http.stream.next(open.id)) {
    seen.push(event.data);
  }
  const page = ctx.http.request({ url: input.url, method: "GET" });
  return `${open.status} ${seen.join("+")} ${page.body}`;
}
"#;

#[test]
fn a_typescript_tool_streams_signs_and_requests_through_the_platform() {
    let tool = parse_source(Path::new("/owner/tools/Streamer.ts"), STREAMING.to_owned(), true).expect("parsed");
    let http = Arc::new(ScriptedHttp::new([(200, "data: a\n\ndata: b\n\n".to_owned()), (200, "page".to_owned())]));
    let mut workspace = Workspace::new("/tmp");
    workspace.services.network = Some(Network(http.clone()));
    let outcome = tool.run(&json!({ "url": "https://api.dev/s" }), &workspace);
    assert_eq!(outcome.content, "200 a+b page", "{outcome:?}");
    let signed = platform::crypto::call("hmac", &json!({ "key": "k", "data": "d", "encoding": "base64" }));
    assert_eq!(http.sent()[0].body, json!({ "signed": signed.unwrap_or_default() }).to_string());
}
