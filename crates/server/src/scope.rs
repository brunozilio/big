use axum::http::{Method, StatusCode};
use domain::principal::Principal;
use domain::workflow::ItemId;
use store::ownership::{APPLICATION, ATTACHMENT, CONNECTION, SESSION};

use crate::auth::settings::{self, Mode};
use crate::error::{ApiError, ApiResult, said};
use crate::state::Shared;

pub fn shared_mode() -> bool {
    settings::mode() == Mode::Shared
}

const CLAIMED_KEY: &str = "auth.claimed_admins";

static CLAIMED: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

fn claimed() -> Vec<String> {
    CLAIMED.read().map(|kept| kept.clone()).unwrap_or_default()
}

pub fn load_claimed(store: &store::Store) {
    let kept: Vec<String> = store
        .preference(CLAIMED_KEY)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    if let Ok(mut held) = CLAIMED.write() {
        *held = kept;
    }
}

pub fn claim_admin(store: &store::Store, login: &str) -> ApiResult<()> {
    let mut kept = claimed();
    if !kept.iter().any(|known| known == login) {
        kept.push(login.to_owned());
    }
    store.set_preference(CLAIMED_KEY, &serde_json::json!(kept))?;
    if let Ok(mut held) = CLAIMED.write() {
        *held = kept;
    }
    Ok(())
}

pub fn configured_admins() -> Vec<String> {
    settings::domains_of(brand::auth::ADMINS)
}

pub fn admins() -> Vec<String> {
    let mut all = configured_admins();
    for login in claimed() {
        if !all.contains(&login) {
            all.push(login);
        }
    }
    all
}

pub fn is_admin(person: &Principal) -> bool {
    !shared_mode() || admins().contains(&person.email)
}

pub fn item_owner(shared: &Shared, id: &str) -> ApiResult<Option<String>> {
    let item = shared.backend.orchestrator.item(&ItemId::from(id))?;
    Ok(item.map(|item| orchestrator::owners::of(&shared.backend.store, &item)))
}

pub fn session_owner(shared: &Shared, session: &str) -> Option<String> {
    let store = &shared.backend.store;
    if let Ok(Some(owner)) = store.recorded_owner(SESSION, session) {
        return Some(owner);
    }
    let items = store.items().ok()?;
    let found = items.iter().find(|item| {
        shared.backend.orchestrator.item_sessions(&item.id).is_ok_and(|all| all.iter().any(|s| s.session == session))
    })?;
    let owner = orchestrator::owners::of(store, found);
    if let Err(error) = store.set_owner(SESSION, session, &owner) {
        tracing::warn!(%error, "the owner of a session could not be kept");
    }
    Some(owner)
}

fn hidden() -> ApiError {
    ApiError::missing(said("There is nothing here.", "No hay nada aquí.", "Não há nada aqui."))
}

pub fn admin_only() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "admin_only",
        said(
            "Only an administrator can change this.",
            "Solo un administrador puede cambiar esto.",
            "Só um administrador pode mudar isso.",
        ),
    )
}

fn segments(path: &str) -> Vec<&str> {
    path.trim_start_matches('/').split('/').collect()
}

fn needs_admin(method: &Method, parts: &[&str]) -> bool {
    let changes = method != Method::GET && method != Method::HEAD;
    match parts {
        ["api", "permissions" | "browser" | "memories"] => changes,
        ["api", "permissions", "discover"] | ["api", "fs", "dirs"] | ["api", "memories", "move"] => true,
        ["api", "items" | "sessions", _, "open"] => true,
        ["api", "admin" | "library", ..] => true,
        _ => false,
    }
}

fn owner_of(shared: &Shared, person: &Principal, parts: &[&str]) -> ApiResult<Option<String>> {
    let store = &shared.backend.store;
    Ok(match parts {
        ["api", "items", id, ..] => item_owner(shared, id)?,
        ["api", "sessions", id, ..] => Some(session_owner(shared, id).unwrap_or_default()),
        ["api", "connections", id] | ["api", "connections", id, "call"] if !id.starts_with(crate::ai::PREFIX) => {
            Some(store.owner_of(CONNECTION, id)?)
        }
        ["api", "connections", id, ..] if id.starts_with(crate::ai::PREFIX) => {
            Some(if is_admin(person) { person.owner().to_owned() } else { String::new() })
        }
        ["api", "applications", id] => Some(store.owner_of(APPLICATION, id)?),
        ["api", "attachments", id] => Some(store.owner_of(ATTACHMENT, id)?),
        _ => None,
    })
}

pub fn check(shared: &Shared, person: &Principal, method: &Method, path: &str) -> ApiResult<()> {
    if !shared_mode() {
        return Ok(());
    }
    let parts = segments(path);
    if needs_admin(method, &parts) && !is_admin(person) {
        return Err(admin_only());
    }
    match owner_of(shared, person, &parts)? {
        Some(owner) if owner != person.owner() => Err(hidden()),
        _ => Ok(()),
    }
}

pub fn live_filter(ctx: &crate::state::Ctx, owner: &str) -> crate::live::Allowed {
    let (ctx, owner) = (std::sync::Arc::clone(ctx), owner.to_owned());
    std::sync::Arc::new(move |told: &crate::live::Told| {
        if let Some(tagged) = &told.owner {
            return *tagged == owner;
        }
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&told.data) else { return false };
        match event["session"].as_str() {
            Some(session) => session_owner(&ctx, session).is_some_and(|found| found == owner),
            None => true,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_for_everyone_need_an_administrator() {
        assert!(!needs_admin(&Method::PUT, &segments("/api/branding")));
        assert!(needs_admin(&Method::PUT, &segments("/api/permissions")));
        assert!(!needs_admin(&Method::GET, &segments("/api/branding")));
        assert!(needs_admin(&Method::GET, &segments("/api/fs/dirs")));
        assert!(needs_admin(&Method::GET, &segments("/api/library/item")));
        assert!(needs_admin(&Method::POST, &segments("/api/items/i-1/open")));
        assert!(!needs_admin(&Method::POST, &segments("/api/items/i-1/rerun")));
    }
}
