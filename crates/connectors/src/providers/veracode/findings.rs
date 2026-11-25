use std::cmp::Ordering;

use serde_json::{Value, json};

use crate::error::ActionError;
use crate::provider::{Args, text_of};

pub const KINDS: [(&str, &str); 4] =
    [("static", "STATIC"), ("dependency", "SCA"), ("dynamic", "DYNAMIC"), ("manual", "MANUAL")];
pub const DEFAULT_LIMIT: u64 = 50;
pub const MOST_LIMIT: u64 = 500;
const MOST_DESCRIPTION: usize = 2000;
const ENTITIES: [(&str, &str); 7] =
    [("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'"), ("&apos;", "'"), ("&nbsp;", " "), ("&amp;", "&")];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub scan_types: Vec<&'static str>,
    pub min_severity: u64,
    pub policy: bool,
    pub open: bool,
    pub limit: usize,
}

fn scan_type_of(kind: &str) -> Result<&'static str, ActionError> {
    let found = KINDS.iter().find(|(name, _)| name.eq_ignore_ascii_case(kind.trim()));
    found.map(|(_, scan)| *scan).ok_or_else(|| {
        let known: Vec<&str> = KINDS.iter().map(|(name, _)| *name).collect();
        ActionError::permanent(format!("{kind:?} is not a kind of finding; use {}", known.join(", ")))
    })
}

impl Filter {
    pub fn of(args: Args<'_>) -> Result<Filter, ActionError> {
        let mut scan_types = Vec::new();
        for kind in args.list("kinds") {
            let scan = scan_type_of(kind.as_str().ok_or_else(|| ActionError::missing("kinds"))?)?;
            if !scan_types.contains(&scan) {
                scan_types.push(scan);
            }
        }
        if scan_types.is_empty() {
            scan_types = KINDS.iter().map(|(_, scan)| *scan).collect();
        }
        let limit = args.optional_number("limit").unwrap_or(DEFAULT_LIMIT).clamp(1, MOST_LIMIT);
        Ok(Filter {
            scan_types,
            min_severity: args.optional_number("minSeverity").unwrap_or(0).min(5),
            policy: args.optional_flag("policy").unwrap_or(false),
            open: args.optional_flag("open").unwrap_or(true),
            limit: usize::try_from(limit).unwrap_or(usize::MAX),
        })
    }

    pub fn keeps(&self, finding: &Value) -> bool {
        let severity = finding["severity"].as_u64().unwrap_or(0);
        severity >= self.min_severity
            && (!self.policy || finding["policy"] == json!(true))
            && (!self.open || finding["status"] == "open")
    }
}

pub fn plain(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => {
                inside = true;
                text.push(' ');
            }
            '>' if inside => inside = false,
            _ if !inside => text.push(c),
            _ => {}
        }
    }
    let decoded = ENTITIES.iter().fold(text, |text, (entity, shown)| text.replace(entity, shown));
    let spaced: Vec<&str> = decoded.split_whitespace().collect();
    spaced.join(" ").chars().take(MOST_DESCRIPTION).collect()
}

fn word_of(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn text_or_null(value: &Value) -> Value {
    word_of(value).map_or(Value::Null, Value::String)
}

fn kind_of(scan: &str) -> &'static str {
    KINDS.iter().find(|(_, known)| known.eq_ignore_ascii_case(scan)).map_or("static", |(kind, _)| *kind)
}

fn cwe_of(details: &Value) -> Value {
    let Some(id) = word_of(&details["cwe"]["id"]) else { return Value::Null };
    json!({ "id": id, "name": text_of(&details["cwe"], "/name") })
}

fn cve_of(details: &Value) -> Value {
    let cve = &details["cve"];
    let Some(id) = word_of(&cve["name"]) else { return Value::Null };
    let cvss = cve["cvss3"]["score"].as_f64().or_else(|| cve["cvss"].as_f64());
    json!({ "id": id, "cvss": cvss })
}

fn is_open(status: &Value) -> bool {
    text_of(status, "/status").eq_ignore_ascii_case("OPEN")
        && !text_of(status, "/resolution_status").eq_ignore_ascii_case("APPROVED")
}

fn title_of(kind: &str, details: &Value, cwe: &Value, cve: &Value) -> String {
    let component = word_of(&details["component_filename"]);
    let weakness = word_of(&cwe["name"]).or_else(|| word_of(&cwe["id"]).map(|id| format!("CWE-{id}")));
    let named = if kind == "dependency" { word_of(&cve["id"]).or(weakness) } else { weakness };
    match (named, component) {
        (Some(named), Some(component)) if kind == "dependency" => format!("{named} in {component}"),
        (Some(named), _) => named,
        (None, Some(component)) => component,
        (None, None) => text_of(details, "/attack_vector"),
    }
}

fn file_of(details: &Value) -> Value {
    text_or_null(&details["file_path"]).as_str().map_or_else(|| text_or_null(&details["file_name"]), |path| json!(path))
}

pub fn normalized(found: &Value) -> Value {
    let details = &found["finding_details"];
    let status = &found["finding_status"];
    let kind = kind_of(&text_of(found, "/scan_type"));
    let cwe = cwe_of(details);
    let cve = cve_of(details);
    json!({
        "id": word_of(&found["issue_id"]).unwrap_or_default(),
        "title": title_of(kind, details, &cwe, &cve),
        "severity": details["severity"].as_u64().unwrap_or(0).min(5),
        "kind": kind,
        "status": if is_open(status) { "open" } else { "closed" },
        "new": status["new"].as_bool().unwrap_or(false),
        "policy": found["violates_policy"].as_bool().unwrap_or(false),
        "cwe": cwe,
        "cve": cve,
        "file": file_of(details),
        "line": details["file_line_number"].as_u64(),
        "module": text_or_null(&details["module"]),
        "component": text_or_null(&details["component_filename"]),
        "version": text_or_null(&details["version"]),
        "description": plain(found["description"].as_str().unwrap_or_default()),
        "found": text_or_null(&status["first_found_date"]),
        "url": text_or_null(&details["url"]),
    })
}

fn by_severity(a: &Value, b: &Value) -> Ordering {
    let severity = |finding: &Value| finding["severity"].as_u64().unwrap_or(0);
    let id = |finding: &Value| text_of(finding, "/id");
    let number = |finding: &Value| id(finding).parse::<u64>().unwrap_or(u64::MAX);
    severity(b).cmp(&severity(a)).then_with(|| number(a).cmp(&number(b))).then_with(|| id(a).cmp(&id(b)))
}

pub fn chosen(raw: &[Value], filter: &Filter) -> Value {
    let mut kept: Vec<Value> = raw.iter().map(normalized).filter(|finding| filter.keeps(finding)).collect();
    kept.sort_by(by_severity);
    kept.dedup_by(|a, b| a["id"] == b["id"] && a["kind"] == b["kind"]);
    kept.truncate(filter.limit);
    Value::Array(kept)
}
