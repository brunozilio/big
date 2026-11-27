use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Local,
    Shared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Password,
    Email,
    Passkey,
    Sso,
}

impl Method {
    pub fn key(self) -> &'static str {
        match self {
            Method::Password => "password",
            Method::Email => "email",
            Method::Passkey => "passkey",
            Method::Sso => "sso",
        }
    }

    fn named(name: &str) -> Option<Method> {
        match name.trim() {
            "password" => Some(Method::Password),
            "email" => Some(Method::Email),
            "passkey" => Some(Method::Passkey),
            "sso" => Some(Method::Sso),
            _ => None,
        }
    }
}

static FORCED: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();

pub fn force(mode: Mode) {
    let _ = FORCED.set(mode);
}

pub fn mode() -> Mode {
    FORCED.get().copied().unwrap_or_else(|| mode_of(brand::auth::MODE))
}

pub fn mode_of(text: &str) -> Mode {
    if text.trim() == "shared" { Mode::Shared } else { Mode::Local }
}

pub fn methods() -> Vec<Method> {
    methods_of(brand::auth::METHODS)
}

pub fn methods_of(text: &str) -> Vec<Method> {
    let mut methods: Vec<Method> = text.split(',').filter_map(Method::named).collect();
    methods.dedup();
    methods
}

pub fn offers(method: Method) -> bool {
    methods().contains(&method) && (method != Method::Sso || sso_ready())
}

pub fn sso_ready() -> bool {
    !brand::auth::SSO_ISSUER.trim().is_empty() && !brand::auth::SSO_CLIENT_ID.trim().is_empty()
}

pub fn allowed_domains() -> Vec<String> {
    domains_of(brand::auth::ALLOWED_DOMAINS)
}

pub fn domains_of(text: &str) -> Vec<String> {
    text.split([',', ' ']).map(|domain| domain.trim().to_lowercase()).filter(|domain| !domain.is_empty()).collect()
}

fn number(text: &str, fallback: i64) -> i64 {
    text.trim().parse().ok().filter(|value: &i64| *value > 0).unwrap_or(fallback)
}

pub fn session_seconds() -> i64 {
    number(brand::auth::SESSION_HOURS, 336) * 3600
}

pub fn code_seconds() -> i64 {
    number(brand::auth::CODE_MINUTES, 10) * 60
}

pub fn login_attempts() -> i64 {
    number(brand::auth::LOGIN_ATTEMPTS, 5)
}

pub fn lock_seconds() -> i64 {
    number(brand::auth::LOCK_MINUTES, 15) * 60
}

static SETUP_FILE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn force_setup_file(path: &str) {
    let _ = SETUP_FILE.set(path.to_owned());
}

pub fn setup_file() -> &'static str {
    SETUP_FILE.get().map_or_else(|| brand::auth::SETUP_FILE.trim(), String::as_str)
}

pub fn code_attempts() -> i64 {
    number(brand::auth::CODE_ATTEMPTS, 5)
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(0))
}

pub fn public_origin(port: u16) -> String {
    origin_of(brand::server::PUBLIC_URL, port)
}

pub fn public_base(port: u16) -> String {
    base_of(brand::server::PUBLIC_URL, port)
}

pub fn base_path() -> String {
    path_of(brand::server::PUBLIC_URL)
}

pub fn base_of(configured: &str, port: u16) -> String {
    let configured = configured.trim().trim_end_matches('/');
    if configured.is_empty() { format!("http://127.0.0.1:{port}") } else { configured.to_owned() }
}

pub fn origin_of(configured: &str, port: u16) -> String {
    let base = base_of(configured, port);
    let path = path_of(&base);
    base.strip_suffix(&path).map_or_else(|| base.clone(), str::to_owned)
}

pub fn path_of(configured: &str) -> String {
    let configured = configured.trim().trim_end_matches('/');
    let rest = configured.split_once("://").map_or("", |(_, rest)| rest);
    rest.find('/').map_or_else(String::new, |start| rest[start..].to_owned())
}

pub fn host_of(origin: &str) -> String {
    let rest = origin.split_once("://").map_or(origin, |(_, rest)| rest);
    let host = rest.split('/').next().unwrap_or(rest);
    match host.rsplit_once(':') {
        Some((name, port)) if port.chars().all(|c| c.is_ascii_digit()) && !name.ends_with(']') => name.to_owned(),
        _ => host.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_shared_turns_the_login_on() {
        assert_eq!(mode_of("shared"), Mode::Shared);
        assert_eq!(mode_of("local"), Mode::Local);
        assert_eq!(mode_of("anything"), Mode::Local);
    }

    #[test]
    fn methods_and_domains_are_lists() {
        assert_eq!(methods_of("email, sso,nope,passkey"), vec![Method::Email, Method::Sso, Method::Passkey]);
        assert_eq!(methods_of("password,passkey"), vec![Method::Password, Method::Passkey]);
        assert_eq!(domains_of("Acme.com, b.io"), vec!["acme.com".to_owned(), "b.io".to_owned()]);
    }

    #[test]
    fn the_public_origin_falls_back_to_the_loopback_port() {
        assert_eq!(origin_of("", 7420), "http://127.0.0.1:7420");
        assert_eq!(origin_of("https://app.acme.com/", 7420), "https://app.acme.com");
        assert_eq!(host_of("https://app.acme.com"), "app.acme.com");
    }

    #[test]
    fn a_public_address_with_a_path_keeps_the_origin_apart_from_the_base() {
        let url = "https://abc.execute-api.us-east-2.amazonaws.com/app/";
        assert_eq!(origin_of(url, 1), "https://abc.execute-api.us-east-2.amazonaws.com");
        assert_eq!(base_of(url, 1), "https://abc.execute-api.us-east-2.amazonaws.com/app");
        assert_eq!(path_of(url), "/app");
        assert_eq!(path_of("https://app.acme.com"), "");
        assert_eq!(path_of(""), "");
        assert_eq!(host_of("http://localhost:7420/x"), "localhost");
    }
}
