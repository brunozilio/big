use std::sync::Arc;

use browser::{BrowserInfo, Hub, Resolved};
use connectors::browser::ConnectorBrowser;
use serde_json::Value;
use store::Store;

#[derive(Debug)]
pub struct Browsers {
    hub: Hub,
    store: Arc<Store>,
    settings: browser::Settings,
}

impl Browsers {
    pub fn new(hub: Hub, store: Arc<Store>, settings: browser::Settings) -> Self {
        Self { hub, store, settings }
    }

    fn device(&self, connection: &str) -> Result<String, String> {
        let chosen = self
            .store
            .connection(connection)
            .map_err(|error| error.to_string())?
            .and_then(|record| record.settings["browser"].as_str().map(str::to_owned));
        if let Some(device) = chosen {
            let browsers = self.hub.visible(&session_of(connection));
            if browsers.iter().any(|browser| browser.device_id == device) {
                return Ok(device);
            }
            return Err("The browser chosen for this connection is not connected. Open Chrome with the extension, or change the connection's browser.".to_owned());
        }
        match self.hub.resolve(&session_of(connection)) {
            Resolved::Browser(device) => Ok(device),
            Resolved::NoneConnected => Err(browser::no_browser()),
            Resolved::Several(browsers) => Err(several(&browsers)),
        }
    }
}

pub const CONNECTOR_PREFIX: &str = "connector:";

pub fn session_of(connection: &str) -> String {
    format!("{CONNECTOR_PREFIX}{connection}")
}

fn several(browsers: &[BrowserInfo]) -> String {
    let listed: Vec<String> =
        browsers.iter().map(|browser| format!("{} ({})", browser.name, browser.device_id)).collect();
    format!(
        "More than one browser is connected: {}. Choose the one this connection uses by setting its browser to a device id above.",
        listed.join(", ")
    )
}

impl ConnectorBrowser for Browsers {
    fn eval(&self, connection: &str, request: &Value) -> Result<Value, String> {
        let device = self.device(connection)?;
        let tab = request["tabId"].as_i64();
        let url = request["url"].as_str();
        let text = request["text"].as_str().ok_or("The browser eval needs the text to run.")?;
        let sites: Vec<String> =
            request["sites"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
        let frame = request.get("frame").filter(|frame| !frame.is_null());
        let session = session_of(connection);
        let eval = browser::Eval { session: &session, tab, url, text, frame, sites: &sites };
        browser::evaluate(&self.hub, &device, &self.settings, &eval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connection_is_its_own_session_and_several_browsers_name_their_ids() {
        assert_eq!(session_of("c1"), "connector:c1");
        let browsers = vec![
            BrowserInfo { device_id: "a".into(), name: "Work".into(), platform: "mac".into(), version: "1".into() },
            BrowserInfo { device_id: "b".into(), name: "Home".into(), platform: "mac".into(), version: "1".into() },
        ];
        let message = several(&browsers);
        assert!(message.contains("Work (a)") && message.contains("Home (b)"));
    }
}
