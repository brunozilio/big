use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSpec {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub extension_to_language: BTreeMap<String, String>,
    #[serde(default)]
    pub root_markers: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<InstallSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallSpec {
    Npm(Vec<String>),
    Go(String),
    Download(DownloadSpec),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadSpec {
    pub program: String,
    pub platforms: BTreeMap<String, Asset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub url: String,
    pub archive: ArchiveKind,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveKind {
    Gzip,
    Zip,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerConfig {
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub extension_to_language: Option<BTreeMap<String, String>>,
    pub root_markers: Option<Vec<String>>,
    pub env: Option<BTreeMap<String, String>>,
    pub disabled: bool,
}

impl ServerConfig {
    pub fn without_commands(self) -> Self {
        Self { command: None, args: None, env: None, ..self }
    }
}

pub fn servers(defaults: Vec<ServerSpec>, layers: &[BTreeMap<String, ServerConfig>]) -> Vec<ServerSpec> {
    let mut specs = defaults;
    for (name, config) in layers.iter().flatten() {
        let at = specs.iter().position(|spec| &spec.name == name);
        if config.disabled {
            if let Some(at) = at {
                specs.remove(at);
            }
            continue;
        }
        match at {
            Some(at) => apply(&mut specs[at], config),
            None => specs.extend(new_server(name, config)),
        }
    }
    specs
}

fn apply(spec: &mut ServerSpec, config: &ServerConfig) {
    let config = config.clone();
    if let Some(command) = config.command
        && command != spec.command
    {
        spec.command = command;
        spec.install = None;
    }
    if let Some(args) = config.args {
        spec.args = args;
    }
    if let Some(languages) = config.extension_to_language {
        spec.extension_to_language = languages;
    }
    if let Some(markers) = config.root_markers {
        spec.root_markers = markers;
    }
    if let Some(env) = config.env {
        spec.env.extend(env);
    }
}

fn new_server(name: &str, config: &ServerConfig) -> Option<ServerSpec> {
    let mut spec = ServerSpec {
        name: name.to_owned(),
        command: config.command.clone()?,
        args: Vec::new(),
        extension_to_language: config.extension_to_language.clone().filter(|map| !map.is_empty())?,
        root_markers: Vec::new(),
        env: BTreeMap::new(),
        install: None,
    };
    apply(&mut spec, config);
    Some(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(json: serde_json::Value) -> BTreeMap<String, ServerConfig> {
        serde_json::from_value(json).expect("layer")
    }

    fn defaults() -> Vec<ServerSpec> {
        serde_json::from_value(serde_json::json!([
            {"name": "rust-analyzer", "command": "rust-analyzer", "extensionToLanguage": {".rs": "rust"},
             "rootMarkers": ["Cargo.toml"], "install": {"go": "example.com/ra@v1"}},
            {"name": "gopls", "command": "gopls", "extensionToLanguage": {".go": "go"}},
            {"name": "clangd", "command": "clangd", "extensionToLanguage": {".c": "c"}}
        ]))
        .expect("specs")
    }

    fn named<'a>(specs: &'a [ServerSpec], name: &str) -> Option<&'a ServerSpec> {
        specs.iter().find(|spec| spec.name == name)
    }

    #[test]
    fn a_setting_changes_a_built_in_server_field_by_field() {
        let specs = servers(
            defaults(),
            &[layer(serde_json::json!({"rust-analyzer": {"command": "/opt/ra", "env": {"RA_LOG": "error"}}}))],
        );
        let rust = named(&specs, "rust-analyzer").expect("rust-analyzer");
        assert_eq!(rust.command, "/opt/ra");
        assert_eq!(rust.env.get("RA_LOG").map(String::as_str), Some("error"));
        assert_eq!(rust.extension_to_language.get(".rs").map(String::as_str), Some("rust"));
        assert_eq!(rust.root_markers, ["Cargo.toml"]);
        assert_eq!(rust.install, None, "a server run by another command is not the one its recipe installs");
    }

    #[test]
    fn a_setting_that_keeps_the_command_keeps_the_install() {
        let specs = servers(defaults(), &[layer(serde_json::json!({"rust-analyzer": {"command": "rust-analyzer"}}))]);
        assert!(named(&specs, "rust-analyzer").and_then(|spec| spec.install.as_ref()).is_some());
    }

    #[test]
    fn a_later_layer_adds_removes_and_wins_over_an_earlier_one() {
        let user = layer(serde_json::json!({
            "gopls": {"disabled": true},
            "zls": {"command": "zls", "extensionToLanguage": {".zig": "zig"}},
            "half": {"command": "half"}
        }));
        let project = layer(serde_json::json!({"zls": {"args": ["--stdio"]}, "clangd": {"disabled": true}}));
        let specs = servers(defaults(), &[user, project]);
        assert!(named(&specs, "gopls").is_none());
        assert!(named(&specs, "clangd").is_none());
        assert!(named(&specs, "half").is_none(), "a new server needs a command and its languages");
        let zig = named(&specs, "zls").expect("zls");
        assert_eq!((zig.command.as_str(), zig.args.as_slice()), ("zls", ["--stdio".to_owned()].as_slice()));
        assert!(named(&specs, "rust-analyzer").is_some());
    }
}
