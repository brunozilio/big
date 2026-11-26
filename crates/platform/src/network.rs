use std::path::PathBuf;

const CA_VARIABLES: [&str; 4] = ["SSL_CERT_FILE", "REQUESTS_CA_BUNDLE", "CURL_CA_BUNDLE", "NODE_EXTRA_CA_CERTS"];
const HTTPS_PROXY: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"];
const HTTP_PROXY: [&str; 4] = ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"];
const NO_PROXY: [&str; 2] = ["NO_PROXY", "no_proxy"];

type Route = fn(String) -> reqwest::Result<reqwest::Proxy>;

#[derive(Debug, Clone, Copy, Default)]
pub struct Configured<'a> {
    pub ca_files: &'a str,
    pub https_proxy: &'a str,
    pub http_proxy: &'a str,
    pub no_proxy: &'a str,
}

impl<'a> Configured<'a> {
    pub fn from_pairs(pairs: &[(&'a str, &'a str)]) -> Configured<'a> {
        let get = |key: &str| pairs.iter().find(|(name, _)| *name == key).map_or("", |(_, value)| *value);
        Configured {
            ca_files: get("ca_files"),
            https_proxy: get("https_proxy"),
            http_proxy: get("http_proxy"),
            no_proxy: get("no_proxy"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub ca_files: Vec<PathBuf>,
    pub https_proxy: Option<String>,
    pub http_proxy: Option<String>,
    pub no_proxy: String,
}

impl Settings {
    pub fn from(configured: Configured<'_>, env: &dyn Fn(&str) -> Option<String>) -> Settings {
        let first = |names: &[&str]| names.iter().find_map(|name| env(name).filter(|value| !value.trim().is_empty()));
        let chosen = |own: &str, names: &[&str]| {
            if own.trim().is_empty() { first(names) } else { Some(own.trim().to_owned()) }
        };
        let mut ca_files: Vec<PathBuf> = listed(configured.ca_files).map(PathBuf::from).collect();
        for found in CA_VARIABLES.iter().filter_map(|name| env(name)) {
            ca_files.extend(listed(&found).map(PathBuf::from));
        }
        ca_files.dedup();
        let no_proxy = chosen(configured.no_proxy, &NO_PROXY).unwrap_or_default();
        Settings {
            ca_files,
            https_proxy: chosen(configured.https_proxy, &HTTPS_PROXY),
            http_proxy: chosen(configured.http_proxy, &HTTP_PROXY),
            no_proxy: bypassed(&no_proxy),
        }
    }

    pub fn current(configured: Configured<'_>) -> Settings {
        Settings::from(configured, &crate::environment::var)
    }

    pub fn shared(configured: Configured<'_>) -> &'static Settings {
        static SHARED: std::sync::OnceLock<Settings> = std::sync::OnceLock::new();
        SHARED.get_or_init(|| Settings::current(configured))
    }

    pub fn applied(&self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        let mut builder = builder.no_proxy();
        for path in &self.ca_files {
            for certificate in certificates(path) {
                builder = builder.add_root_certificate(certificate);
            }
        }
        let routes: [(&Option<String>, Route); 2] =
            [(&self.https_proxy, reqwest::Proxy::https::<String>), (&self.http_proxy, reqwest::Proxy::http::<String>)];
        for (address, route) in routes {
            let Some(address) = address else { continue };
            match route(address.clone()) {
                Ok(proxy) => builder = builder.proxy(proxy.no_proxy(reqwest::NoProxy::from_string(&self.no_proxy))),
                Err(error) => tracing::warn!(proxy = %hidden(address), %error, "network.proxy.ignored"),
            }
        }
        builder
    }

    pub fn proxied(&self) -> bool {
        self.https_proxy.is_some() || self.http_proxy.is_some()
    }
}

fn listed(text: &str) -> impl Iterator<Item = &str> {
    text.split([',', ';']).map(str::trim).filter(|part| !part.is_empty())
}

pub fn bypassed(raw: &str) -> String {
    let entries: Vec<&str> =
        listed(raw).map(|entry| if entry == "*" { entry } else { entry.trim_start_matches('*') }).collect();
    entries.join(",")
}

fn certificates(path: &std::path::Path) -> Vec<reqwest::Certificate> {
    let read = std::fs::read(path).map_err(|error| error.to_string());
    let parsed = read.and_then(|bytes| {
        reqwest::Certificate::from_pem_bundle(&bytes)
            .or_else(|_| reqwest::Certificate::from_der(&bytes).map(|one| vec![one]))
            .map_err(|error| error.to_string())
    });
    match parsed {
        Ok(found) => found,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "network.certificate.ignored");
            Vec::new()
        }
    }
}

fn hidden(address: &str) -> String {
    match url::Url::parse(address) {
        Ok(mut parsed) if !parsed.username().is_empty() || parsed.password().is_some() => {
            let _ = parsed.set_username("");
            let _ = parsed.set_password(None);
            parsed.to_string()
        }
        _ => address.to_owned(),
    }
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod tests;
