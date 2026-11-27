use std::sync::Arc;

use store::Store;

use crate::error::Error;

pub(crate) async fn stored<T, F>(store: &Arc<Store>, work: F) -> Result<T, Error>
where
    T: Send + 'static,
    F: FnOnce(&Store) -> store::Result<T> + Send + 'static,
{
    let store = Arc::clone(store);
    match tokio::task::spawn_blocking(move || work(&store)).await {
        Ok(done) => Ok(done?),
        Err(error) => Err(Error::Sink(format!("the store work stopped: {error}"))),
    }
}
