use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Markers {
    pub open: String,
    pub close: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CallFormat {
    #[default]
    Json,
    Xml,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct CallMarkers {
    pub open: String,
    pub close: String,
    #[serde(default)]
    pub format: CallFormat,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Format {
    #[serde(default)]
    pub when: String,
    #[serde(default)]
    pub think: Option<Markers>,
    #[serde(default)]
    pub call: Option<CallMarkers>,
}

impl Format {
    pub fn chosen(formats: &[Format], template: &str) -> Format {
        formats.iter().find(|format| template.contains(&format.when)).cloned().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Reasoning(String),
    Content(String),
    Call { name: String, arguments: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Text,
    Think,
    Call,
}

#[derive(Debug)]
pub struct Splitter {
    format: Format,
    tools: Value,
    region: Region,
    held: String,
    call: String,
    stops: Vec<String>,
    stopped: bool,
    fresh: bool,
}

fn partial(text: &str, tag: &str) -> usize {
    (1..tag.len())
        .rev()
        .find(|&n| text.len() >= n && text.is_char_boundary(text.len() - n) && tag.starts_with(&text[text.len() - n..]))
        .unwrap_or(0)
}

impl Splitter {
    pub fn new(format: Format, tools: Value, thinking: bool, stops: Vec<String>) -> Splitter {
        let region = if thinking && format.think.is_some() { Region::Think } else { Region::Text };
        Splitter { format, tools, region, held: String::new(), call: String::new(), stops, stopped: false, fresh: true }
    }

    pub fn stopped(&self) -> bool {
        self.stopped
    }

    fn tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self.stops.iter().filter(|stop| !stop.is_empty()).cloned().collect();
        match self.region {
            Region::Text => {
                tags.extend(self.format.think.as_ref().map(|think| think.open.clone()));
                tags.extend(self.format.call.as_ref().map(|call| call.open.clone()));
            }
            Region::Think => tags.extend(self.format.think.as_ref().map(|think| think.close.clone())),
            Region::Call => tags.extend(self.format.call.as_ref().map(|call| call.close.clone())),
        }
        tags.retain(|tag| !tag.is_empty());
        tags
    }

    pub fn push(&mut self, text: &str, out: &mut Vec<Piece>) {
        if self.stopped {
            return;
        }
        let mut rest = std::mem::take(&mut self.held);
        rest.push_str(text);
        loop {
            let tags = self.tags();
            let found = tags.iter().filter_map(|tag| rest.find(tag.as_str()).map(|at| (at, tag.clone()))).min();
            if let Some((at, tag)) = found {
                let before = rest[..at].to_owned();
                self.emit(&before, out);
                rest = rest[at + tag.len()..].to_owned();
                if self.stops.contains(&tag) && self.region != Region::Call {
                    self.stopped = true;
                    return;
                }
                self.cross(&tag, out);
                self.fresh = true;
                continue;
            }
            let keep = tags.iter().map(|tag| partial(&rest, tag)).max().unwrap_or(0);
            let (now, later) = rest.split_at(rest.len() - keep);
            self.emit(now, out);
            self.held = later.to_owned();
            return;
        }
    }

    fn cross(&mut self, tag: &str, out: &mut Vec<Piece>) {
        let think = self.format.think.clone().unwrap_or_default();
        let call = self.format.call.clone().unwrap_or_default();
        self.region = match self.region {
            Region::Text if tag == think.open => Region::Think,
            Region::Text if tag == call.open => Region::Call,
            Region::Think if tag == think.close => Region::Text,
            Region::Call if tag == call.close => {
                let body = std::mem::take(&mut self.call);
                out.extend(parsed(&body, call.format, &self.tools));
                Region::Text
            }
            same => same,
        };
    }

    fn emit(&mut self, text: &str, out: &mut Vec<Piece>) {
        if text.is_empty() {
            return;
        }
        let text = if self.fresh && self.region != Region::Call { text.trim_start_matches('\n') } else { text };
        if text.is_empty() {
            return;
        }
        self.fresh = false;
        match self.region {
            Region::Text => out.push(Piece::Content(text.to_owned())),
            Region::Think => out.push(Piece::Reasoning(text.to_owned())),
            Region::Call => self.call.push_str(text),
        }
    }

    pub fn finish(&mut self, out: &mut Vec<Piece>) {
        let held = std::mem::take(&mut self.held);
        self.emit(&held, out);
        if self.region == Region::Call {
            let body = std::mem::take(&mut self.call);
            let format = self.format.call.as_ref().map(|call| call.format).unwrap_or_default();
            out.extend(parsed(&body, format, &self.tools));
        }
    }
}

pub fn parsed(body: &str, format: CallFormat, tools: &Value) -> Option<Piece> {
    match format {
        CallFormat::Json => json_call(body),
        CallFormat::Xml => xml_call(body, tools),
    }
}

fn json_call(body: &str) -> Option<Piece> {
    let value: Value = serde_json::from_str(body.trim()).ok()?;
    let name = value["name"].as_str()?.to_owned();
    let arguments = match &value["arguments"] {
        Value::String(text) => text.clone(),
        Value::Null => "{}".to_owned(),
        other => other.to_string(),
    };
    Some(Piece::Call { name, arguments })
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<(&'a str, &'a str)> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some((&text[start..end], &text[end + close.len()..]))
}

fn kind_of(tools: &Value, function: &str, parameter: &str) -> String {
    let declared = tools.as_array().into_iter().flatten().find(|tool| tool["function"]["name"] == function);
    let schema = declared.map_or(&Value::Null, |tool| &tool["function"]["parameters"]["properties"][parameter]);
    schema["type"].as_str().unwrap_or_default().to_owned()
}

fn trimmed(value: &str) -> &str {
    let value = value.strip_prefix('\n').unwrap_or(value);
    value.strip_suffix('\n').unwrap_or(value)
}

fn xml_call(body: &str, tools: &Value) -> Option<Piece> {
    let (name, mut rest) = between(body, "<function=", ">")?;
    let name = name.trim().to_owned();
    let mut arguments = Map::new();
    while let Some((key, after)) = between(rest, "<parameter=", ">") {
        let end = after.find("</parameter>").unwrap_or(after.find("</function>").unwrap_or(after.len()));
        let raw = trimmed(&after[..end]);
        let value = if kind_of(tools, &name, key.trim()) == "string" {
            Value::String(raw.to_owned())
        } else {
            serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_owned()))
        };
        arguments.insert(key.trim().to_owned(), value);
        rest = after.get(end..).unwrap_or_default();
    }
    Some(Piece::Call { name, arguments: Value::Object(arguments).to_string() })
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
