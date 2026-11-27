use serde::Deserialize;
use serde_json::Value;

const FILE: &str = "mcp-resources.ts";
const EXPORT: &str = "resources";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Serve {
    Host {
        host: String,
        #[serde(default)]
        tools: Vec<String>,
    },
    Connector {
        connector: String,
        #[serde(default)]
        actions: Vec<String>,
        #[serde(default = "writes")]
        writes: bool,
    },
}

fn writes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Resource {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub serves: Vec<Serve>,
}

fn kept(found: Option<Value>) -> Vec<Resource> {
    let listed = found.and_then(|found| found.as_array().cloned()).unwrap_or_default();
    let mut kept: Vec<Resource> = Vec::new();
    for entry in listed {
        let Ok(mut resource) = serde_json::from_value::<Resource>(entry) else { continue };
        resource.name = resource.name.trim().to_ascii_lowercase();
        if resource.name.is_empty() || kept.iter().any(|known| known.name == resource.name) {
            continue;
        }
        kept.push(resource);
    }
    kept
}

pub fn declared_in(root: Option<&std::path::Path>) -> Vec<Resource> {
    kept(content::value_in(root, FILE, EXPORT))
}

pub fn declared() -> Vec<Resource> {
    declared_in(content::folder().as_deref())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn names(declared: &[Resource]) -> String {
        declared.iter().map(|resource| resource.name.as_str()).collect::<Vec<_>>().join(", ")
    }

    #[test]
    fn the_built_in_content_declares_what_each_resource_serves() {
        let declared = declared_in(None);
        assert_eq!(names(&declared), "browser, memory, memory-read, connectors");
        let read = &declared[2].serves[0];
        assert_eq!(read, &Serve::Host { host: "memory".into(), tools: vec!["memory_search".into()] });
        let connectors = &declared[3].serves[0];
        assert_eq!(connectors, &Serve::Connector { connector: "*".into(), actions: vec![], writes: false });
    }

    #[test]
    fn a_broken_or_repeated_declaration_is_left_out() {
        let found = json!([
            { "name": " Mine ", "serves": [{ "host": "memory" }] },
            { "name": "mine", "serves": [] },
            { "name": "", "serves": [] },
            { "serves": [] },
            { "name": "jira", "serves": [{ "connector": "jira", "actions": ["tracker"] }] }
        ]);
        let declared = kept(Some(found));
        assert_eq!(names(&declared), "mine, jira");
        assert_eq!(
            declared[1].serves[0],
            Serve::Connector { connector: "jira".into(), actions: vec!["tracker".into()], writes: true }
        );
    }
}
