use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Ordered,
    Task,
}

#[derive(Debug, Default)]
struct Writer {
    blocks: Vec<Value>,
    paragraph: Vec<String>,
    list: Option<(ListKind, Vec<Value>)>,
    ids: usize,
}

impl Writer {
    fn next_id(&mut self, prefix: &str) -> String {
        self.ids += 1;
        format!("{prefix}-{}", self.ids)
    }

    fn flush_paragraph(&mut self) {
        if self.paragraph.is_empty() {
            return;
        }
        let mut content = Vec::new();
        for (index, line) in std::mem::take(&mut self.paragraph).iter().enumerate() {
            if index > 0 {
                content.push(json!({"type": "hardBreak"}));
            }
            content.extend(inline(line));
        }
        self.blocks.push(json!({"type": "paragraph", "content": content}));
    }

    fn flush_list(&mut self) {
        let Some((kind, items)) = self.list.take() else { return };
        let block = match kind {
            ListKind::Bullet => json!({"type": "bulletList", "content": items}),
            ListKind::Ordered => json!({"type": "orderedList", "content": items}),
            ListKind::Task => json!({"type": "taskList", "attrs": {"localId": self.next_id("list")}, "content": items}),
        };
        self.blocks.push(block);
    }

    fn flush(&mut self) {
        self.flush_paragraph();
        self.flush_list();
    }

    fn block(&mut self, block: Value) {
        self.flush();
        self.blocks.push(block);
    }

    fn list_item(&mut self, kind: ListKind, item: Value) {
        self.flush_paragraph();
        if self.list.as_ref().is_some_and(|(open, _)| *open != kind) {
            self.flush_list();
        }
        self.list.get_or_insert_with(|| (kind, Vec::new())).1.push(item);
    }
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    let rest = line[level..].strip_prefix(' ')?;
    (1..=6).contains(&level).then_some((level, rest.trim()))
}

fn task(line: &str) -> Option<(bool, &str)> {
    let rest = line.strip_prefix("- [").or_else(|| line.strip_prefix("* ["))?;
    let (mark, text) = rest.split_once("] ")?;
    match mark {
        " " => Some((false, text)),
        "x" | "X" => Some((true, text)),
        _ => None,
    }
}

fn bullet(line: &str) -> Option<&str> {
    line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).or_else(|| line.strip_prefix("+ "))
}

fn ordered(line: &str) -> Option<&str> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0).then(|| line[digits..].strip_prefix(". ")).flatten()
}

fn paragraph_item(text: &str) -> Value {
    json!({"type": "listItem", "content": [{"type": "paragraph", "content": inline(text)}]})
}

pub fn markdown_to_adf(markdown: &str) -> Value {
    let mut writer = Writer::default();
    let mut lines = markdown.lines();
    while let Some(raw) = lines.next() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if let Some(language) = trimmed.strip_prefix("```") {
            let code: Vec<&str> = lines.by_ref().take_while(|next| !next.trim_start().starts_with("```")).collect();
            let mut block = json!({"type": "codeBlock", "content": []});
            if !language.trim().is_empty() {
                block["attrs"] = json!({"language": language.trim()});
            }
            if !code.is_empty() {
                block["content"] = json!([{"type": "text", "text": code.join("\n")}]);
            }
            writer.block(block);
        } else if trimmed.is_empty() {
            writer.flush();
        } else if let Some((level, text)) = heading(trimmed) {
            writer.block(json!({"type": "heading", "attrs": {"level": level}, "content": inline(text)}));
        } else if trimmed == "---" || trimmed == "***" {
            writer.block(json!({"type": "rule"}));
        } else if let Some(quote) = trimmed.strip_prefix("> ").or_else(|| trimmed.strip_prefix('>')) {
            writer.block(
                json!({"type": "blockquote", "content": [{"type": "paragraph", "content": inline(quote.trim())}]}),
            );
        } else if let Some((done, text)) = task(trimmed) {
            let id = writer.next_id("task");
            let state = if done { "DONE" } else { "TODO" };
            let item = json!({"type": "taskItem", "attrs": {"localId": id, "state": state}, "content": inline(text)});
            writer.list_item(ListKind::Task, item);
        } else if let Some(text) = bullet(trimmed) {
            writer.list_item(ListKind::Bullet, paragraph_item(text));
        } else if let Some(text) = ordered(trimmed) {
            writer.list_item(ListKind::Ordered, paragraph_item(text));
        } else {
            writer.flush_list();
            writer.paragraph.push(trimmed.to_owned());
        }
    }
    writer.flush();
    json!({"type": "doc", "version": 1, "content": writer.blocks})
}

fn text_node(text: &str, marks: &[Value]) -> Value {
    if marks.is_empty() {
        json!({"type": "text", "text": text})
    } else {
        json!({"type": "text", "text": text, "marks": marks})
    }
}

fn with_mark(nodes: Vec<Value>, mark: &Value) -> Vec<Value> {
    nodes
        .into_iter()
        .map(|mut node| {
            if let Some(marks) = node.get_mut("marks").and_then(Value::as_array_mut) {
                marks.push(mark.clone());
            } else {
                node["marks"] = json!([mark]);
            }
            node
        })
        .collect()
}

fn link_at(rest: &str) -> Option<(&str, &str, usize)> {
    let close = rest.find("](")?;
    let end = rest[close..].find(')')? + close;
    let label = &rest[1..close];
    let href = &rest[close + 2..end];
    (!label.is_empty() && !href.is_empty() && !href.contains(char::is_whitespace)).then_some((label, href, end + 1))
}

fn delimited<'a>(rest: &'a str, delimiter: &str) -> Option<(&'a str, usize)> {
    let inner = rest.strip_prefix(delimiter)?;
    let end = inner.find(delimiter)?;
    (end > 0).then(|| (&inner[..end], delimiter.len() * 2 + end))
}

fn styled(rest: &str) -> Option<(Vec<Value>, usize)> {
    if let Some((code, used)) = delimited(rest, "`") {
        return Some((vec![text_node(code, &[json!({"type": "code"})])], used));
    }
    if rest.starts_with('[')
        && let Some((label, href, used)) = link_at(rest)
    {
        return Some((with_mark(inline(label), &json!({"type": "link", "attrs": {"href": href}})), used));
    }
    for (delimiter, mark) in [("**", "strong"), ("*", "em")] {
        if let Some((inner, used)) = delimited(rest, delimiter) {
            return Some((with_mark(inline(inner), &json!({"type": mark})), used));
        }
    }
    None
}

pub fn inline(text: &str) -> Vec<Value> {
    let mut nodes = Vec::new();
    let mut plain = String::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let starts_style = rest.starts_with(['`', '[', '*']);
        if let Some((styled_nodes, used)) = starts_style.then(|| styled(rest)).flatten() {
            if !plain.is_empty() {
                nodes.push(text_node(&std::mem::take(&mut plain), &[]));
            }
            nodes.extend(styled_nodes);
            at += used;
        } else if let Some(c) = rest.chars().next() {
            plain.push(c);
            at += c.len_utf8();
        }
    }
    if !plain.is_empty() {
        nodes.push(text_node(&plain, &[]));
    }
    nodes
}

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
