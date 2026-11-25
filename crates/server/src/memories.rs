use std::path::{Path, PathBuf};
use std::sync::Arc;

use app::api::memory::MemoryListing;
use app::{ApplicationOf, ApplicationRef, MemoryShelf};
use axum::Json;
use axum::extract::State;
use axum::response::Response;
use domain::{MemoryEdit, MemoryId};
use serde::Deserialize;
use serde_json::{Value, json};
use store::{Application, Store};

use crate::error::{ApiError, ApiResult, json as reply, said};
use crate::state::Ctx;

#[derive(Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
enum Shelf {
    Repository { directory: String },
    Application { id: String },
    Global,
}

#[derive(Deserialize)]
struct Change {
    shelf: Shelf,
    edit: MemoryEdit,
}

#[derive(Deserialize)]
struct Move {
    from: Shelf,
    id: MemoryId,
    to: Shelf,
}

fn same_folder(directory: &str, checkout: &Path) -> bool {
    Path::new(directory.trim_end_matches('/')) == checkout
}

pub fn applications(store: Arc<Store>) -> ApplicationOf {
    Arc::new(move |checkout: &Path| {
        let applications = store.applications().ok()?;
        let owner =
            applications.into_iter().find(|it| it.repositories.iter().any(|r| same_folder(&r.directory, checkout)))?;
        Some(ApplicationRef { id: owner.id, name: owner.name })
    })
}

fn unknown() -> ApiError {
    ApiError::missing(said(
        "There is no such memory shelf: name a repository or an application you added, or the global memory.",
        "No existe ese lugar de memoria: nombra un repositorio o una aplicación registrada, o la memoria global.",
        "Esse lugar de memória não existe: indique um repositório ou uma aplicação cadastrada, ou a memória global.",
    ))
}

fn checked(store: &Store, shelf: Shelf) -> ApiResult<MemoryShelf> {
    let applications = store.applications()?;
    let known = match &shelf {
        Shelf::Global => true,
        Shelf::Application { id } => applications.iter().any(|it| &it.id == id),
        Shelf::Repository { directory } => {
            applications.iter().flat_map(|it| &it.repositories).any(|r| &r.directory == directory)
        }
    };
    if !known {
        return Err(unknown());
    }
    Ok(match shelf {
        Shelf::Global => MemoryShelf::Global,
        Shelf::Application { id } => MemoryShelf::Application(id),
        Shelf::Repository { directory } => MemoryShelf::Repository(PathBuf::from(directory)),
    })
}

fn kept(listing: &MemoryListing) -> Value {
    json!({ "memories": listing.memories, "forgotten": listing.forgotten })
}

fn application(client: &app::Client, application: &Application) -> Value {
    let repositories: Vec<Value> = application
        .repositories
        .iter()
        .filter(|repository| !repository.directory.is_empty())
        .map(|repository| {
            let directory = PathBuf::from(&repository.directory);
            let mut listed = kept(&client.memory_shelf(&MemoryShelf::Repository(directory.clone())));
            listed["directory"] = json!(repository.directory);
            listed["team"] = json!(client.team_memories(&directory));
            listed
        })
        .collect();
    let mut listed = kept(&client.memory_shelf(&MemoryShelf::Application(application.id.clone())));
    listed["id"] = json!(application.id);
    listed["name"] = json!(application.name);
    listed["repositories"] = json!(repositories);
    listed
}

fn tree(client: &app::Client, store: &Store) -> ApiResult<Value> {
    let applications: Vec<Value> = store.applications()?.iter().map(|it| application(client, it)).collect();
    Ok(json!({ "global": kept(&client.memory_shelf(&MemoryShelf::Global)), "applications": applications }))
}

fn parsed<T: for<'de> Deserialize<'de>>(body: Value) -> ApiResult<T> {
    serde_json::from_value(body).map_err(|_| {
        ApiError::bad(said(
            "Send the shelf ({ scope: repository | application | global }) and what to do with the memory.",
            "Envía el lugar ({ scope: repository | application | global }) y qué hacer con la memoria.",
            "Envie o lugar ({ scope: repository | application | global }) e o que fazer com a memória.",
        ))
    })
}

pub async fn list(State(ctx): State<Ctx>) -> ApiResult<Response> {
    Ok(reply(tree(&ctx.backend.client, &ctx.backend.store)?))
}

pub async fn edit(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    let change: Change = parsed(body)?;
    let shelf = checked(&ctx.backend.store, change.shelf)?;
    ctx.backend.client.edit_memory(&shelf, change.edit).map_err(ApiError::failed)?;
    Ok(reply(tree(&ctx.backend.client, &ctx.backend.store)?))
}

pub async fn moved(State(ctx): State<Ctx>, Json(body): Json<Value>) -> ApiResult<Response> {
    let wanted: Move = parsed(body)?;
    let from = checked(&ctx.backend.store, wanted.from)?;
    let to = checked(&ctx.backend.store, wanted.to)?;
    ctx.backend.client.move_memory(&from, wanted.id, &to).map_err(ApiError::bad)?;
    Ok(reply(tree(&ctx.backend.client, &ctx.backend.store)?))
}

#[cfg(test)]
mod tests {
    use store::Repository;

    use super::*;

    fn store() -> Arc<Store> {
        let store = Store::in_memory(0).expect("store");
        let repository = Repository { directory: "/work/rocket".into(), ..Repository::default() };
        let application = Application {
            id: "a".into(),
            name: "Rocket".into(),
            repositories: vec![repository],
            ..Application::default()
        };
        store.save_application(&application).expect("save");
        Arc::new(store)
    }

    #[test]
    fn only_shelves_the_user_added_can_be_read_or_changed() {
        let store = store();
        let shelf = |body: Value| checked(&store, serde_json::from_value(body).expect("shelf"));
        assert_eq!(shelf(json!({ "scope": "global" })).ok(), Some(MemoryShelf::Global));
        assert_eq!(
            shelf(json!({ "scope": "application", "id": "a" })).ok(),
            Some(MemoryShelf::Application("a".into()))
        );
        let rocket = shelf(json!({ "scope": "repository", "directory": "/work/rocket" })).ok();
        assert_eq!(rocket, Some(MemoryShelf::Repository("/work/rocket".into())));
        assert!(shelf(json!({ "scope": "repository", "directory": "/etc" })).is_err());
        assert!(shelf(json!({ "scope": "application", "id": "b" })).is_err());
    }

    #[test]
    fn a_repository_belongs_to_the_application_that_lists_its_folder() {
        let of = applications(store());
        assert_eq!(of(Path::new("/work/rocket")).map(|it| it.name), Some("Rocket".into()));
        assert_eq!(of(Path::new("/work/other")), None);
    }

    #[test]
    fn a_change_names_its_shelf_and_what_to_do() {
        let change: Change =
            parsed(json!({ "shelf": { "scope": "global" }, "edit": { "op": "forget", "id": 3 } })).expect("change");
        assert!(matches!(change.shelf, Shelf::Global));
        assert_eq!(change.edit, MemoryEdit::Forget { id: MemoryId(3) });
    }
}
