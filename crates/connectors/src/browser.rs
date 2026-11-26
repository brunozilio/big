use std::sync::{Arc, OnceLock};

use serde_json::Value;

pub trait ConnectorBrowser: Send + Sync + std::fmt::Debug {
    fn eval(&self, connection: &str, request: &Value) -> Result<Value, String>;
}

static BROWSER: OnceLock<Arc<dyn ConnectorBrowser>> = OnceLock::new();

pub fn install(browser: Arc<dyn ConnectorBrowser>) {
    let _ = BROWSER.set(browser);
}

pub fn installed() -> Option<&'static Arc<dyn ConnectorBrowser>> {
    BROWSER.get()
}
