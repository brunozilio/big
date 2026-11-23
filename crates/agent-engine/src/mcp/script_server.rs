use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use domain::mcp::{McpEndpoint, respond, text_result};
use serde_json::{Value, json};

use super::ServerSpec;
use crate::tools::script_io::StandaloneHost;
use crate::tools::script_tool::{Capabilities, Declaration};
pub use crate::tools::script_tool::{Origin, ScriptAccess};

#[derive(Debug, Clone)]
pub struct Granted {
    pub access: ScriptAccess,
    pub origin: Origin,
}

pub fn found(folder: &Path, cwd: &Path, granted: &Granted) -> Vec<ServerSpec> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut servers: Vec<ServerSpec> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "js"))
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            Some(ServerSpec::Script {
                name,
                path,
                cwd: cwd.to_path_buf(),
                access: granted.access,
                origin: granted.origin,
            })
        })
        .collect();
    servers.sort_by(|a, b| a.name().cmp(b.name()));
    servers
}

pub struct ScriptServer {
    name: String,
    path: PathBuf,
    source: String,
    cwd: PathBuf,
    access: ScriptAccess,
    origin: Origin,
    starter: crate::tools::Starter,
    busy: std::time::Duration,
    tools: Vec<Value>,
    capabilities: Capabilities,
}

impl std::fmt::Debug for ScriptServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptServer").field("name", &self.name).field("path", &self.path).finish_non_exhaustive()
    }
}

impl ScriptServer {
    pub fn open(name: &str, path: &Path, cwd: &Path, access: ScriptAccess) -> Result<Self, String> {
        let source = std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let shown = path.display().to_string();
        let exported = script::exported(&shown, &source, "default").map_err(|error| format!("{shown}: {error}"))?;
        let tools = exported["tools"]
            .as_object()
            .ok_or_else(|| format!("{shown}: export default {{ tools: {{ … }} }}"))?
            .iter()
            .map(|(tool, spec)| {
                json!({
                    "name": tool,
                    "description": spec["description"].as_str().unwrap_or_default(),
                    "inputSchema": spec.get("input").cloned().unwrap_or_else(|| json!({ "type": "object" })),
                })
            })
            .collect();
        let capabilities = Capabilities::listed(exported.get("capabilities"), Declaration::ServerExport)
            .map_err(|error| format!("{shown}: {error}"))?;
        let (name, path, cwd) = (name.to_owned(), path.to_path_buf(), cwd.to_path_buf());
        let starter = crate::tools::Starter::default();
        let busy = script::MOST_BUSY;
        Ok(Self { name, path, source, cwd, access, origin: Origin::Project, starter, busy, tools, capabilities })
    }

    #[cfg(test)]
    #[must_use]
    pub fn busy_for(mut self, busy: std::time::Duration) -> Self {
        self.busy = busy;
        self
    }

    #[must_use]
    pub fn coming_from(mut self, origin: Origin) -> Self {
        self.origin = origin;
        self
    }

    fn call(&self, tool: &str, arguments: &Value) -> Value {
        if !self.tools.iter().any(|known| known["name"] == tool) {
            return text_result(format!("{} has no tool {tool}", self.name), true);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let host: Arc<dyn script::Host> = Arc::new(StandaloneHost {
            cwd: self.cwd.clone(),
            access: self.access,
            origin: self.origin,
            starter: self.starter.clone(),
            stop: Arc::clone(&stop),
            cancel: crate::model::CancellationToken::new(),
            script: format!("the MCP server {}", self.name),
            capabilities: self.capabilities.clone(),
        });
        let data = json!({ "cwd": self.cwd.display().to_string(), "server": self.name });
        let export = format!("default.tools.{tool}.run");
        let shown = self.path.display().to_string();
        let arguments = std::slice::from_ref(arguments);
        let limit = script::Limit::Busy(self.busy);
        match script::run_limited(&shown, &self.source, &export, arguments, &data, host, stop, limit) {
            Ok(Value::String(text)) => text_result(text, false),
            Ok(Value::Null) => text_result("", false),
            Ok(value) => text_result(value.to_string(), false),
            Err(error) => text_result(error, true),
        }
    }
}

impl McpEndpoint for ScriptServer {
    fn handle(&self, message: &Value) -> Option<Value> {
        respond(message, &self.name, || self.tools.clone(), |tool, arguments| Ok(self.call(tool, arguments)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_javascript_server_lists_and_runs_its_tools() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("math.js");
        std::fs::write(
            &file,
            r#"export default { tools: {
  add: { description: "Adds a and b.", input: { type: "object" }, run: (input) => String(input.a + input.b) },
  fail: { run: () => { throw new Error("nope") } },
} };"#,
        )
        .expect("server");
        let server = ScriptServer::open("math", &file, dir.path(), ScriptAccess::default()).expect("open");
        let listed = server.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).expect("answer");
        assert_eq!(listed["result"]["tools"][0]["name"], "add");
        let call = |name: &str| json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": name, "arguments": { "a": 2, "b": 3 } } });
        let added = server.handle(&call("add")).expect("answer");
        assert_eq!(added["result"]["content"][0]["text"], "5");
        let failed = server.handle(&call("fail")).expect("answer");
        assert_eq!(failed["result"]["isError"], true);
        let granted = Granted { access: ScriptAccess::default(), origin: Origin::Owner };
        assert_eq!(found(dir.path(), dir.path(), &granted).len(), 1);
    }

    #[test]
    fn a_javascript_server_declares_what_it_calls_and_runs_only_that() {
        let dir = tempfile::tempdir().expect("dir");
        let call = |tool: &str| json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": {} } });
        let said = |server: &ScriptServer, tool: &str| {
            server.handle(&call(tool)).expect("answer")["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        };
        let undeclared = dir.path().join("undeclared.js");
        std::fs::write(
            &undeclared,
            r#"export default { tools: { go: { run: (input, ctx) => ctx.exec("touch made.txt").code } } };"#,
        )
        .expect("server");
        let server = ScriptServer::open("undeclared", &undeclared, dir.path(), ScriptAccess::default()).expect("open");
        let refused = said(&server, "go");
        assert!(refused.contains("did not declare ctx.exec") && refused.contains("capabilities: [...]"), "{refused}");

        let declared = dir.path().join("declared.js");
        std::fs::write(
            &declared,
            r#"export default { capabilities: ["exec"], tools: { go: { run: (input, ctx) => String(ctx.exec("touch made.txt").code) } } };"#,
        )
        .expect("server");
        let server = ScriptServer::open("declared", &declared, dir.path(), ScriptAccess::default()).expect("open");
        assert_eq!(said(&server, "go"), "0", "a declared call runs");
        assert!(dir.path().join("made.txt").exists());
        assert!(ScriptServer::open("bad", &write_bad(dir.path()), dir.path(), ScriptAccess::default()).is_err());
    }

    fn write_bad(dir: &Path) -> PathBuf {
        let bad = dir.join("bad.js");
        std::fs::write(&bad, r#"export default { capabilities: ["root"], tools: {} };"#).expect("server");
        bad
    }

    #[test]
    fn a_javascript_server_that_never_ends_is_stopped() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("loop.js");
        std::fs::write(&file, "export default { tools: { spin: { run: () => { for (;;) {} } } } };").expect("server");
        let server = ScriptServer::open("loop", &file, dir.path(), ScriptAccess::default())
            .expect("open")
            .busy_for(std::time::Duration::from_millis(100));
        let call =
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "spin", "arguments": {} } });
        let answer = server.handle(&call).expect("answer");
        assert_eq!(answer["result"]["isError"], true);
        assert!(
            answer["result"]["content"][0]["text"].as_str().is_some_and(|t| t.contains("running time")),
            "{answer}"
        );
    }

    #[test]
    fn a_javascript_server_keeps_to_the_scripts_settings() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("notes.txt"), "kept").expect("notes");
        let file = dir.path().join("files.js");
        std::fs::write(
            &file,
            r#"export default { capabilities: ["readFile"], tools: { read: { run: (input, ctx) => ctx.readFile("notes.txt") } } };"#,
        )
        .expect("server");
        let call =
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "read", "arguments": {} } });
        let open = ScriptServer::open("files", &file, dir.path(), ScriptAccess::default()).expect("open");
        assert_eq!(open.handle(&call).expect("answer")["result"]["content"][0]["text"], "kept");
        let closed = ScriptAccess { files: false, ..ScriptAccess::default() };
        let refused =
            ScriptServer::open("files", &file, dir.path(), closed).expect("open").handle(&call).expect("answer");
        assert_eq!(refused["result"]["isError"], true);
        assert!(
            refused["result"]["content"][0]["text"].as_str().is_some_and(|t| t.contains("scripts.files")),
            "{refused}"
        );
    }
}
