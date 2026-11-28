use axum::http::{HeaderMap, StatusCode};
use domain::mcp::{LocalMcp, McpPicked};
use orchestrator::mcp_resources::{Resource, Serve};

use super::{EXTERNAL, Refusal};
use crate::state::Ctx;

const ORIGIN: &str = "MCP";
pub const BROWSER: &str = "browser";
pub const MEMORY: &str = "memory";
const HOSTED: &[&str] = &[BROWSER, MEMORY];
const PROJECT: &str = "project";
const CONNECTION: &str = "connection";
const INPUTS: &[&str] = &[PROJECT, CONNECTION];

pub fn declared() -> Vec<Resource> {
    orchestrator::mcp_resources::declared()
}

pub fn serves_only(resource: &Resource, host: &str) -> bool {
    matches!(resource.serves.as_slice(), [Serve::Host { host: served, tools }] if served == host && tools.is_empty())
}

fn names(declared: &[Resource]) -> String {
    declared.iter().map(|resource| resource.name.as_str()).collect::<Vec<_>>().join(", ")
}

pub fn described(resources: &[&Resource]) -> String {
    let line = |resource: &&Resource| match resource.description.trim() {
        "" => format!("- {}", resource.name),
        description => format!("- {}: {description}", resource.name),
    };
    resources.iter().map(line).collect::<Vec<_>>().join("\n")
}

pub fn wanted<'a>(given: &str, declared: &'a [Resource]) -> Result<Vec<&'a Resource>, Refusal> {
    let mut found: Vec<&Resource> = Vec::new();
    for name in given.split([',', ' ']).map(str::trim).filter(|name| !name.is_empty()) {
        let name = name.to_ascii_lowercase();
        let Some(resource) = declared.iter().find(|resource| resource.name == name) else {
            let told = format!("There is no resource `{name}`; the resources are {}.", names(declared));
            return Err((StatusCode::BAD_REQUEST, told));
        };
        if !found.iter().any(|kept| kept.name == resource.name) {
            found.push(resource);
        }
    }
    if found.is_empty() {
        let all: Vec<&Resource> = declared.iter().collect();
        let told =
            format!("Send the {} header with the resources you want:\n{}", super::resources_header(), described(&all));
        return Err((StatusCode::BAD_REQUEST, told));
    }
    Ok(found)
}

pub struct Asked<'a> {
    pub ctx: &'a Ctx,
    pub headers: &'a HeaderMap,
    pub id: &'a str,
    pub runtime: tokio::runtime::Handle,
}

impl Asked<'_> {
    pub fn parts(&self, resource: &Resource) -> Result<Vec<LocalMcp>, Refusal> {
        self.check(resource)?;
        let chosen = super::connection(self.headers).ok();
        let mut parts = Vec::new();
        let mut connected = false;
        for serve in &resource.serves {
            match serve {
                Serve::Host { host, tools } => parts.push(McpPicked::local(self.host(host)?, tools.clone())),
                Serve::Connector { connector, actions, writes } => {
                    connected = true;
                    let wanted =
                        super::connector::Wanted { provider: connector, actions, writes: *writes, connection: chosen };
                    parts.extend(super::connector::local(self.ctx, &wanted, self.runtime.clone(), self.id));
                }
            }
        }
        if let Some(chosen) = chosen.filter(|_| connected && parts.is_empty()) {
            let told = format!("No connection named `{chosen}` serves the resource `{}`.", resource.name);
            return Err((StatusCode::BAD_REQUEST, told));
        }
        Ok(parts)
    }

    fn check(&self, resource: &Resource) -> Result<(), Refusal> {
        for need in &resource.needs {
            match need.trim() {
                PROJECT => drop(super::project(self.headers)?),
                CONNECTION => drop(super::connection(self.headers)?),
                other => {
                    let told = format!(
                        "The resource `{}` needs `{other}`, which the app cannot give; it gives {}.",
                        resource.name,
                        INPUTS.join(", ")
                    );
                    return Err((StatusCode::INTERNAL_SERVER_ERROR, told));
                }
            }
        }
        Ok(())
    }

    fn host(&self, name: &str) -> Result<LocalMcp, Refusal> {
        match name {
            BROWSER => self.browser(),
            MEMORY => Ok(self.ctx.backend.client.memory_mcp(&super::project(self.headers)?, ORIGIN)),
            _ => {
                let told = format!(
                    "The content asks for `{name}`, which the app does not serve; it serves {}.",
                    HOSTED.join(", ")
                );
                Err((StatusCode::INTERNAL_SERVER_ERROR, told))
            }
        }
    }

    fn browser(&self) -> Result<LocalMcp, Refusal> {
        let ctx = self.ctx;
        if !crate::browsers::enabled(&ctx.backend.store) {
            return Err((StatusCode::SERVICE_UNAVAILABLE, "The browser tools are turned off in the app.".to_owned()));
        }
        let settings = browser::Settings::configured(&ctx.backend.data);
        let roots = vec![ctx.backend.attachments(), settings.files.clone()];
        let session = format!("{EXTERNAL}:{}", self.id);
        Ok(browser::Session::new(ctx.backend.browser.clone(), session, settings, roots).local())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resource_that_serves_only_a_whole_host_stands_for_it() {
        let host = |tools: Vec<String>| Serve::Host { host: BROWSER.into(), tools };
        let resource = |serves| Resource { name: "b".into(), description: String::new(), needs: vec![], serves };
        assert!(serves_only(&resource(vec![host(vec![])]), BROWSER));
        assert!(!serves_only(&resource(vec![host(vec!["navigate".into()])]), BROWSER));
        assert!(!serves_only(&resource(vec![host(vec![]), host(vec![])]), BROWSER));
        assert!(!serves_only(&resource(vec![host(vec![])]), MEMORY));
    }

    #[test]
    fn the_header_names_declared_resources_once_each() {
        let resource =
            |name: &str| Resource { name: name.into(), description: String::new(), needs: vec![], serves: vec![] };
        let declared = vec![resource("memory"), resource("browser")];
        let found = wanted("Memory, browser memory", &declared).expect("named");
        assert_eq!(found.iter().map(|resource| resource.name.as_str()).collect::<Vec<_>>(), ["memory", "browser"]);
        let (status, told) = wanted("skills", &declared).expect_err("unknown");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(told.contains("memory, browser"), "{told}");
        let (_, told) = wanted("", &declared).expect_err("none named");
        assert!(told.contains(super::super::resources_header()) && told.contains("- memory\n- browser"), "{told}");
        let mut memory = declared[0].clone();
        memory.description = "Search memory.".into();
        assert_eq!(described(&[&memory, &declared[1]]), "- memory: Search memory.\n- browser");
    }
}
