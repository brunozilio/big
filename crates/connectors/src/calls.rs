use domain::connection::{ConnectionId, ProviderKey};
use domain::origin::Attribution;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Called<'a> {
    pub connection: &'a ConnectionId,
    pub provider: &'a ProviderKey,
    pub action: &'a str,
    pub attribution: Option<&'a Attribution>,
    pub ok: bool,
}

pub trait CallSink: Send + Sync + std::fmt::Debug {
    fn called(&self, call: &Called<'_>);
}

#[cfg(test)]
pub mod kept {
    use std::sync::Mutex;

    use super::*;

    pub type Row = (String, String, Option<Attribution>, bool);

    #[derive(Debug, Default)]
    pub struct Kept(pub Mutex<Vec<Row>>);

    impl CallSink for Kept {
        fn called(&self, call: &Called<'_>) {
            let row = (call.connection.to_string(), call.action.to_owned(), call.attribution.cloned(), call.ok);
            self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(row);
        }
    }
}
