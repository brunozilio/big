use domain::principal::{LOCAL_USER, OWNER_FIELD, owner_in};
use domain::workflow::WorkItem;
use store::Store;

const DEEPEST: usize = 16;

pub fn of(store: &Store, item: &WorkItem) -> String {
    let mut current = item.clone();
    for _ in 0..DEEPEST {
        if current.input[OWNER_FIELD].is_string() {
            return owner_in(&current.input).to_owned();
        }
        let Some(parent) = current.parent.clone() else { break };
        match store.item(&parent) {
            Ok(Some(found)) => current = found,
            _ => break,
        }
    }
    LOCAL_USER.to_owned()
}
