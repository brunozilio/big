use axum::http::{Method, StatusCode};
use domain::principal::Principal;
use domain::workflow::ItemId;
use store::ownership::{APPLICATION, ATTACHMENT, CONNECTION, SESSION};

use crate::auth::settings::{self, Mode};
use crate::error::{ApiError, ApiResult, said};
use crate::roles::{Grant, Grants, Roles};
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

pub fn roles() -> &'static Roles {
    static ROLES: std::sync::OnceLock<Roles> = std::sync::OnceLock::new();
    ROLES.get_or_init(Roles::configured)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub roles: Vec<String>,
    pub grants: Grants,
}

pub fn standing_from(roles: &Roles, listed_admin: bool, kept: &[String]) -> Standing {
    let held = roles.held(listed_admin, kept);
    let grants = if listed_admin { Grants::everything() } else { roles.grants(&held) };
    Standing { roles: held, grants }
}

pub fn standing(shared: &Shared, person: &Principal) -> Standing {
    if !shared_mode() {
        return Standing { roles: roles().held(true, &[]), grants: Grants::everything() };
    }
    let kept = match shared.backend.store.roles_of(&person.user) {
        Ok(held) => held.into_iter().map(|held| held.role).collect(),
        Err(error) => {
            tracing::warn!(%error, "the roles of a person could not be read");
            Vec::new()
        }
    };
    standing_from(roles(), admins().contains(&person.email), &kept)
}

pub fn is_admin(shared: &Shared, person: &Principal) -> bool {
    !shared_mode() || standing(shared, person).grants.has(Grant::Admin)
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

pub fn not_allowed() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "role_denied",
        said("Your role does not allow this.", "Tu rol no permite esto.", "Seu papel não permite isso."),
    )
}

fn segments(path: &str) -> Vec<&str> {
    path.trim_start_matches('/').split('/').collect()
}

fn needed(method: &Method, parts: &[&str]) -> Option<Grant> {
    let changes = method != Method::GET && method != Method::HEAD;
    match parts {
        ["api", "permissions" | "browser" | "memories"] if changes => Some(Grant::Admin),
        ["api", "permissions", "discover"] | ["api", "fs", "dirs"] | ["api", "memories", "move"] => Some(Grant::Admin),
        ["api", "items" | "sessions", _, "open"] | ["api", "agents", _, "kill"] => Some(Grant::Admin),
        ["api", "admin", ..] => Some(Grant::Admin),
        ["api", "library", ..] => Some(Grant::Author),
        ["api", "items" | "sessions" | "btw" | "connections" | "applications" | "attachments", ..] if changes => {
            Some(Grant::Use)
        }
        _ => None,
    }
}

fn refused(grant: Grant) -> ApiError {
    if grant == Grant::Admin { admin_only() } else { not_allowed() }
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
            Some(if is_admin(shared, person) { person.owner().to_owned() } else { String::new() })
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
    if let Some(grant) = needed(method, &parts)
        && !standing(shared, person).grants.has(grant)
    {
        return Err(refused(grant));
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

    fn needs_admin(method: &Method, path: &str) -> bool {
        needed(method, &segments(path)) == Some(Grant::Admin)
    }

    #[test]
    fn settings_for_everyone_need_an_administrator() {
        assert!(!needs_admin(&Method::PUT, "/api/branding"));
        assert!(needs_admin(&Method::PUT, "/api/permissions"));
        assert!(!needs_admin(&Method::GET, "/api/branding"));
        assert!(needs_admin(&Method::GET, "/api/fs/dirs"));
        assert!(needs_admin(&Method::POST, "/api/items/i-1/open"));
        assert!(!needs_admin(&Method::POST, "/api/items/i-1/rerun"));
        assert!(!needs_admin(&Method::POST, "/api/items/i-1/kill"));
        assert!(needs_admin(&Method::POST, "/api/agents/dev/kill"));
        assert!(needs_admin(&Method::GET, "/api/admin/audit"));
    }

    #[test]
    fn content_needs_an_author_and_running_needs_use() {
        assert_eq!(needed(&Method::GET, &segments("/api/library/item")), Some(Grant::Author));
        assert_eq!(needed(&Method::PUT, &segments("/api/library/item")), Some(Grant::Author));
        assert_eq!(needed(&Method::POST, &segments("/api/items")), Some(Grant::Use));
        assert_eq!(needed(&Method::POST, &segments("/api/items/i-1/rerun")), Some(Grant::Use));
        assert_eq!(needed(&Method::POST, &segments("/api/sessions/s-1/messages")), Some(Grant::Use));
        assert_eq!(needed(&Method::GET, &segments("/api/items")), None);
        assert_eq!(needed(&Method::PUT, &segments("/api/onboarding")), None);
    }

    fn configured() -> Roles {
        let access = crate::roles::Access {
            default_role: "member",
            admin_role: "admin",
            sso_claim: "groups",
            sso_groups: "uso=member, autores=author",
        };
        Roles::of(&[("admin", "use author admin"), ("author", "use author"), ("member", "use")], &access)
    }

    fn allowed(standing: &Standing, method: Method, path: &str) -> bool {
        needed(&method, &segments(path)).is_none_or(|grant| standing.grants.has(grant))
    }

    #[test]
    fn someone_in_the_use_group_runs_workflows_but_does_not_edit_content() {
        let roles = configured();
        let from_sso = roles.roles_in(&serde_json::json!({ "groups": ["uso"] }));
        let standing = standing_from(&roles, false, &from_sso);
        assert_eq!(standing.roles, ["member"]);
        assert!(allowed(&standing, Method::POST, "/api/items"));
        assert!(allowed(&standing, Method::POST, "/api/items/i-1/rerun"));
        assert!(allowed(&standing, Method::POST, "/api/items/i-1/steps/review/input"));
        assert!(!allowed(&standing, Method::PUT, "/api/library/item"));
        assert!(!allowed(&standing, Method::POST, "/api/library/item"));
        assert!(!allowed(&standing, Method::GET, "/api/admin/people"));
    }

    #[test]
    fn an_author_edits_content_and_a_listed_admin_keeps_everything() {
        let roles = configured();
        let author = standing_from(&roles, false, &["author".to_owned()]);
        assert!(allowed(&author, Method::PUT, "/api/library/item"));
        assert!(!allowed(&author, Method::PUT, "/api/permissions"));
        let admin = standing_from(&roles, true, &[]);
        assert!(allowed(&admin, Method::PUT, "/api/permissions"));
        assert!(allowed(&admin, Method::PUT, "/api/library/item"));
    }

    #[test]
    fn a_new_person_has_the_access_a_non_admin_had() {
        let standing = standing_from(&configured(), false, &[]);
        for (method, path) in
            [(Method::POST, "/api/items"), (Method::PUT, "/api/branding"), (Method::GET, "/api/usage")]
        {
            assert!(allowed(&standing, method, path));
        }
        for (method, path) in
            [(Method::GET, "/api/library"), (Method::PUT, "/api/memories"), (Method::GET, "/api/fs/dirs")]
        {
            assert!(!allowed(&standing, method, path));
        }
    }

    #[test]
    fn a_role_without_use_only_reads() {
        let roles = Roles::of(
            &[("viewer", "")],
            &crate::roles::Access { default_role: "viewer", admin_role: "", sso_claim: "", sso_groups: "" },
        );
        let standing = standing_from(&roles, false, &[]);
        assert!(allowed(&standing, Method::GET, "/api/items"));
        assert!(!allowed(&standing, Method::POST, "/api/items"));
    }
}
