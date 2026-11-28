pub mod brief;
pub mod route;

use serde_json::Value;
use store::{Application, Store};

fn live(store: &Store, input: &Value) -> Option<Option<Application>> {
    let id = crate::application::id_of(input)?;
    store.application(&id).ok()
}
