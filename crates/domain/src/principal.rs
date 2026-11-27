use serde::{Deserialize, Serialize};

pub const LOCAL_USER: &str = "local";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(pub String);

impl UserId {
    pub fn local() -> Self {
        Self(LOCAL_USER.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_local(&self) -> bool {
        self.0 == LOCAL_USER
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub user: UserId,
    pub email: String,
    pub name: String,
}

impl Principal {
    pub fn local(email: &str) -> Self {
        Self { user: UserId::local(), email: email.to_owned(), name: String::new() }
    }

    pub fn owner(&self) -> &str {
        self.user.as_str()
    }
}

pub fn normalized_login(text: &str) -> Option<String> {
    let login = text.trim().to_lowercase();
    let fine = (1..=64).contains(&login.chars().count())
        && login.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'));
    fine.then_some(login)
}

pub fn normalized_email(text: &str) -> Option<String> {
    let email = text.trim().to_lowercase();
    let (local, domain) = email.split_once('@')?;
    let fine = !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(char::is_whitespace)
        && !domain.contains('@');
    fine.then_some(email)
}

pub const OWNER_FIELD: &str = "owner";

pub fn owner_in(input: &serde_json::Value) -> &str {
    input[OWNER_FIELD].as_str().filter(|owner| !owner.is_empty()).unwrap_or(LOCAL_USER)
}

pub fn domain_of(email: &str) -> &str {
    email.rsplit_once('@').map_or("", |(_, domain)| domain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_trimmed_lowercased_and_need_a_dotted_domain() {
        assert_eq!(normalized_email("  Ana@Example.COM "), Some("ana@example.com".to_owned()));
        assert_eq!(normalized_email("ana@example"), None);
        assert_eq!(normalized_email("@example.com"), None);
        assert_eq!(normalized_email("a b@example.com"), None);
        assert_eq!(normalized_email("a@b@example.com"), None);
        assert_eq!(domain_of("ana@example.com"), "example.com");
    }

    #[test]
    fn an_item_without_an_owner_belongs_to_the_local_user() {
        assert_eq!(owner_in(&serde_json::json!({})), LOCAL_USER);
        assert_eq!(owner_in(&serde_json::json!({ "owner": "u-1" })), "u-1");
    }

    #[test]
    fn the_local_principal_owns_the_local_user() {
        let local = Principal::local("");
        assert!(local.user.is_local());
        assert_eq!(local.owner(), LOCAL_USER);
    }
}
