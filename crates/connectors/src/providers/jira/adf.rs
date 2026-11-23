use serde_json::Value;

fn children(node: &Value) -> &[Value] {
    node.get("content").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn marked(text: &str, marks: &[Value]) -> String {
    marks.iter().fold(text.to_owned(), |inner, mark| match mark.get("type").and_then(Value::as_str) {
        Some("code") => format!("`{inner}`"),
        Some("strong") => format!("**{inner}**"),
        Some("em") => format!("*{inner}*"),
        Some("strike") => format!("~~{inner}~~"),
        Some("link") => format!("[{inner}]({})", mark["attrs"]["href"].as_str().unwrap_or_default()),
        _ => inner,
    })
}

fn inline_markdown(nodes: &[Value]) -> String {
    nodes
        .iter()
        .map(|node| match node.get("type").and_then(Value::as_str) {
            Some("text") => {
                let marks = node.get("marks").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
                marked(node["text"].as_str().unwrap_or_default(), marks)
            }
            Some("hardBreak") => "\n".to_owned(),
            Some("mention") => node["attrs"]["text"].as_str().unwrap_or_default().to_owned(),
            Some("emoji") => {
                node["attrs"]["text"].as_str().or(node["attrs"]["shortName"].as_str()).unwrap_or_default().to_owned()
            }
            Some("inlineCard") => node["attrs"]["url"].as_str().unwrap_or_default().to_owned(),
            _ => inline_markdown(children(node)),
        })
        .collect()
}

fn list_markdown(node: &Value, marker: &dyn Fn(usize, &Value) -> String) -> String {
    children(node)
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let text = if item["type"] == "taskItem" {
                inline_markdown(children(item))
            } else {
                children(item).iter().map(block_markdown).collect::<Vec<_>>().join("\n")
            };
            format!("{}{}", marker(index, item), text.trim_end())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn block_markdown(node: &Value) -> String {
    match node.get("type").and_then(Value::as_str) {
        Some("paragraph") => inline_markdown(children(node)),
        Some("heading") => {
            let level = node["attrs"]["level"].as_u64().unwrap_or(1).clamp(1, 6);
            format!("{} {}", "#".repeat(usize::try_from(level).unwrap_or(1)), inline_markdown(children(node)))
        }
        Some("bulletList") => list_markdown(node, &|_, _| "- ".to_owned()),
        Some("orderedList") => list_markdown(node, &|index, _| format!("{}. ", index + 1)),
        Some("taskList") => list_markdown(node, &|_, item| {
            if item["attrs"]["state"] == "DONE" { "- [x] ".to_owned() } else { "- [ ] ".to_owned() }
        }),
        Some("codeBlock") => {
            let language = node["attrs"]["language"].as_str().unwrap_or_default();
            format!("```{language}\n{}\n```", inline_markdown(children(node)))
        }
        Some("blockquote") => {
            children(node).iter().map(|child| format!("> {}", block_markdown(child))).collect::<Vec<_>>().join("\n")
        }
        Some("rule") => "---".to_owned(),
        Some("text" | "hardBreak" | "mention" | "emoji" | "inlineCard") => inline_markdown(std::slice::from_ref(node)),
        _ => children(node).iter().map(block_markdown).collect::<Vec<_>>().join("\n\n"),
    }
}

pub fn adf_to_markdown(document: &Value) -> String {
    match document.get("type").and_then(Value::as_str) {
        Some("doc") => children(document)
            .iter()
            .map(block_markdown)
            .filter(|block| !block.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        Some(_) => block_markdown(document),
        None => document.as_str().unwrap_or_default().to_owned(),
    }
}

#[cfg(test)]
#[path = "adf_tests.rs"]
mod tests;
