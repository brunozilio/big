use serde_json::{Map, Value, json};

pub const EXTENSIONS: [&str; 3] = ["ts", "js", "md"];

pub fn scripted(path: &str) -> bool {
    path.ends_with(".ts") || path.ends_with(".js")
}

pub fn stem(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.rsplit_once('.').map_or(file, |(stem, _)| stem)
}

pub fn markdown(path: &str, text: &str, runs_from: &str) -> Result<String, String> {
    if !scripted(path) {
        return Ok(text.to_owned());
    }
    let mut spec = script::exported(path, text, "spec")?;
    let body = script::exported(path, text, "default")?;
    let body = body.as_str().ok_or_else(|| format!("{path}: the default export must be the agent's text"))?;
    for export in ["hooks", "intercept"] {
        let outline = script::outline(path, text, export)?;
        let inline = inline(&outline, export, runs_from).map_err(|e| format!("{path}: {e}"))?;
        if !inline.is_empty() {
            joined(&mut spec, export, inline).map_err(|e| format!("{path}: {e}"))?;
        }
    }
    let header = if spec.is_null() { String::new() } else { spec.to_string() };
    Ok(format!("---\n{header}\n---\n{body}"))
}

fn inline(outline: &Value, export: &str, runs_from: &str) -> Result<Map<String, Value>, String> {
    let Some(sections) = outline.as_object() else {
        return if outline.is_null() { Ok(Map::new()) } else { Err(format!("`{export}` must be an object")) };
    };
    let mut written = Map::new();
    for (section, entries) in sections {
        let mut listed = Vec::new();
        for entry in crate::intercept::entries(entries, &format!("{export}.{section}"))? {
            let run = format!("{runs_from}#{}", entry.export);
            let matcher = entry.matcher.unwrap_or_else(|| Value::String(String::new()));
            let mut handler = if export == "hooks" {
                json!({ "type": "script", "command": run })
            } else {
                json!({ "run": run, "matcher": matcher })
            };
            if let Some(timeout) = entry.timeout {
                handler["timeout"] = timeout;
            }
            listed.push(if export == "hooks" { json!({ "matcher": matcher, "hooks": [handler] }) } else { handler });
        }
        written.insert(section.clone(), Value::Array(listed));
    }
    Ok(written)
}

fn joined(spec: &mut Value, export: &str, inline: Map<String, Value>) -> Result<(), String> {
    if spec.is_null() {
        *spec = json!({});
    }
    let spec = spec.as_object_mut().ok_or("`spec` must be an object")?;
    let declared = spec.entry(export).or_insert_with(|| json!({}));
    let declared = declared.as_object_mut().ok_or_else(|| format!("`spec.{export}` must be an object"))?;
    for (section, entries) in inline {
        let listed = declared.entry(section).or_insert_with(|| json!([]));
        let listed = listed.as_array_mut().ok_or_else(|| format!("each entry of `spec.{export}` must be a list"))?;
        listed.extend(entries.as_array().into_iter().flatten().cloned());
    }
    Ok(())
}

pub fn header(text: &str) -> Value {
    crate::prompt::markdown::frontmatter(text).map(|(front, _)| front).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scripted_agent_reads_like_a_written_one() {
        let source = "export const spec = { name: \"dev\", tools: [\"Read\", \"mcp__*\"], avatar: { color: \"#3b82f6\" } };\nconst body: string = `Você é o dev.\\n\\`ok\\``;\nexport default body;";
        let text = markdown("agents/dev.ts", source, "agents/dev.ts").expect("markdown");
        let (front, body) = crate::prompt::markdown::frontmatter(&text).expect("header");
        assert_eq!(front["name"], "dev");
        assert_eq!(front["tools"][1], "mcp__*");
        assert_eq!(front["avatar"]["color"], "#3b82f6");
        assert_eq!(body, "Você é o dev.\n`ok`");
        assert_eq!(header(&text)["name"], "dev");
        assert_eq!(
            markdown("agents/dev.md", "---\nname: x\n---\ny", "agents/dev.md").expect("same"),
            "---\nname: x\n---\ny"
        );
        assert_eq!(stem("agents/dev.ts"), "dev");
        assert!(markdown("agents/bad.ts", "export const spec = {};", "agents/bad.ts").is_err());
    }

    const GUARDED: &str = r#"export const spec = { name: "guarded", hooks: { Stop: [{ hooks: [{ type: "command", command: "true" }] }] } };
export const hooks = {
  PreToolUse: [{ matcher: "Bash", timeout: 5, run(input: any) {
    return input.tool_input.command.includes("rm") ? { decision: "block", reason: `no rm for ${input.agent_type ?? "anyone"}` } : null;
  } }],
  PostToolUse: () => ({ systemMessage: "seen" }),
};
export const intercept = {
  model: (request: any, ctx: any, next: any) => next(request),
  tool: [{ matcher: "Read", run: (input: any, ctx: any, next: any) => next(input) }],
};
export default "Guarda.";"#;

    #[test]
    fn an_agent_writes_its_hooks_as_functions_beside_the_ones_its_spec_names() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("guarded.ts");
        std::fs::write(&file, GUARDED).expect("agent");
        let shown = file.display().to_string();
        let text = markdown(&shown, GUARDED, &shown).expect("markdown");
        let front = header(&text);
        assert_eq!(front["hooks"]["PreToolUse"][0]["hooks"][0]["command"], format!("{shown}#hooks.PreToolUse.0.run"));
        assert_eq!(front["hooks"]["PostToolUse"][0]["hooks"][0]["command"], format!("{shown}#hooks.PostToolUse"));
        assert_eq!(front["hooks"]["Stop"][0]["hooks"][0]["type"], "command");
        assert_eq!(front["intercept"]["model"][0]["run"], format!("{shown}#intercept.model"));
        assert_eq!(front["intercept"]["tool"][0]["run"], format!("{shown}#intercept.tool.0.run"));
        let config = crate::tools::subagent_config::AgentConfig::parse(&front).expect("config");
        assert_eq!(config.intercept.tool[0].matcher, "Read");
        let agent_level = crate::intercept::Level::Agent;
        assert!(config.intercept.model.iter().chain(&config.intercept.tool).all(|one| one.level == agent_level));
        let workspace = crate::tools::Workspace::new(dir.path());
        let input = |command: &str| json!({ "tool_name": "Bash", "tool_input": { "command": command } });
        let pre = crate::hooks::HookEvent::PreToolUse;
        let blocked = config.hooks.run(pre, "Bash", input("rm -rf x"), &workspace).expect("ran");
        assert!(blocked.blocked.is_some_and(|reason| reason.starts_with("no rm")));
        assert_eq!(config.hooks.run(pre, "Bash", input("ls"), &workspace).expect("ran").blocked, None);
        assert_eq!(config.hooks.run(pre, "Read", input("rm"), &workspace).expect("ran").blocked, None);
        let post = crate::hooks::HookEvent::PostToolUse;
        assert_eq!(config.hooks.run(post, "Read", input("ls"), &workspace).expect("ran").context, "seen");
    }

    #[test]
    fn an_inline_hook_that_is_not_a_function_is_named() {
        let source =
            "export const spec = {};\nexport const hooks = { Stop: [{ matcher: \"x\" }] };\nexport default \"x\";";
        let error = markdown("agents/x.ts", source, "agents/x.ts").expect_err("refused");
        assert!(error.contains("hooks.Stop.0: each entry is a function"), "{error}");
    }
}
