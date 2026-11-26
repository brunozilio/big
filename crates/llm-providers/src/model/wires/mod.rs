pub mod chat;
pub mod cloud_code;
pub mod event_stream;
pub mod messages;
pub mod responses;

use std::sync::Arc;

use domain::ModelOption;

use crate::model::ModelApi;
use crate::model::http::Http;

type Open = fn(Arc<dyn Http>, Option<&ModelOption>) -> Arc<dyn ModelApi>;

const ALL: &[(&str, Open)] = &[
    (chat::NAME, chat::open),
    (messages::NAME, messages::open),
    (responses::NAME, responses::open),
    (event_stream::NAME, event_stream::open),
    (cloud_code::NAME, cloud_code::open),
];

pub fn open(name: &str, http: Arc<dyn Http>, model: Option<&ModelOption>) -> Option<Arc<dyn ModelApi>> {
    ALL.iter().find(|(known, _)| *known == name).map(|(_, open)| open(http, model))
}

#[cfg(test)]
mod tests {
    use super::ALL;

    #[test]
    fn the_registered_wires_are_these_in_this_order() {
        let names: Vec<&str> = ALL.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["chat", "messages", "responses", "event_stream", "cloud_code"]);
    }
}
