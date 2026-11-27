use axum::http::{HeaderMap, StatusCode};
use domain::mcp::{LocalMcp, McpPicked};
use orchestrator::mcp_resources::{Resource, Serve};

use super::{EXTERNAL, Refusal};
use crate::state::Ctx;

const ORIGIN: &str = "MCP";
pub const BROWSER: &str = "browser";
pub const MEMORY: &str = "memory";
const HOSTED: &[&str] = &[BROWSER, MEMORY];

pub fn declared() -> Vec<Resource> {
    orchestrator::mcp_resources::declared()
}

fn names(declared: &[Resource]) -> String {
    declared.iter().map(|resource| resource.name.as_str()).collect::<Vec<_>>().join(", ")
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
        let told =
            format!("Send the {} header with the resources you want: {}.", super::resources_header(), names(declared));
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
        let mut parts = Vec::new();
        for serve in &resource.serves {
            match serve {
                Serve::Host { host, tools } => parts.push(McpPicked::local(self.host(host)?, tools.clone())),
                Serve::Connector { connector, actions, writes } => {
                    let wanted = super::connector::Wanted { provider: connector, actions, writes: *writes };
                    parts.extend(super::connector::local(self.ctx, &wanted, self.runtime.clone()));
                }
            }
        }
        Ok(parts)
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
    fn the_header_names_declared_resources_once_each() {
        let resource = |name: &str| Resource { name: name.into(), description: String::new(), serves: vec![] };
        let declared = vec![resource("memory"), resource("browser")];
        let found = wanted("Memory, browser memory", &declared).expect("named");
        assert_eq!(found.iter().map(|resource| resource.name.as_str()).collect::<Vec<_>>(), ["memory", "browser"]);
        let (status, told) = wanted("skills", &declared).expect_err("unknown");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(told.contains("memory, browser"), "{told}");
        let (_, told) = wanted("", &declared).expect_err("none named");
        assert!(told.contains(super::super::resources_header()), "{told}");
    }
}
