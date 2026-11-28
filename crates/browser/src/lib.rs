mod batch;
mod eval;
mod extension;
mod files;
mod find;
mod hub;
mod logic;
mod recording;
mod session;
mod socket;
mod spec;
mod stream;

pub use eval::{Eval, run as evaluate};
pub use extension::{Installed, Packaged, install, packaged};
pub use logic::{Logic, ROUTE as LOGIC_ROUTE, logic};
pub use hub::{BrowserInfo, Hub, OwnerOf, Resolved, no_browser};
pub use session::{Ask, Session};
pub use socket::{ROUTE, connect};
pub use spec::{server_name, tools};
pub use stream::{hello, open, reply};

#[derive(Debug, Clone)]
pub struct Settings {
    pub call: std::time::Duration,
    pub pairing: std::time::Duration,
    pub upload_limit: u64,
    pub files: std::path::PathBuf,
}

impl Settings {
    pub fn configured(data: &std::path::Path) -> Self {
        let number = |text: &str, default: u64| text.trim().parse().unwrap_or(default);
        Self {
            call: std::time::Duration::from_secs(number(brand::browser::CALL_SECONDS, 150)),
            pairing: std::time::Duration::from_secs(number(brand::browser::SWITCH_SECONDS, 120)),
            upload_limit: number(brand::browser::UPLOAD_LIMIT_MB, 10) * 1024 * 1024,
            files: data.join(brand::browser::FILES_FOLDER),
        }
    }
}
