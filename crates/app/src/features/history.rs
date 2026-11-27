use std::path::PathBuf;
use std::sync::Arc;

use agent_engine::history::{History, Hit, Kept, Place, Query, Read, Session, Shelf};
use agent_engine::sessions::scoped_id;

use crate::catalog::SessionCatalog;
use crate::data_root::DataRoot;

const CURRENT: &str = "current";

pub struct KeptHistory {
    root: DataRoot,
    catalog: Arc<dyn SessionCatalog>,
}

impl KeptHistory {
    pub fn new(root: DataRoot, catalog: Arc<dyn SessionCatalog>) -> Self {
        Self { root, catalog }
    }

    fn shelf(&self) -> Shelf {
        let mut kept = vec![Kept { name: CURRENT.to_owned(), dir: self.root.sessions() }];
        kept.extend(self.root.former().into_iter().map(|(name, _, sessions)| Kept { name, dir: sessions }));
        self.catalog.read().listed().iter().fold(Shelf::new(kept), |shelf, listed| {
            let id = scoped_id(&listed.cwd, &listed.session).0;
            shelf.named(id, listed.title.clone(), listed.cwd.display().to_string())
        })
    }
}

fn own_folder() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join(brand::DIR))
}

impl History for KeptHistory {
    fn places(&self) -> Vec<Place> {
        let mut places = vec![
            Place::new("data", self.root.data(), "everything this computer keeps for the app"),
            Place::new(
                "sessions",
                self.root.sessions(),
                "one <id>.jsonl per conversation, chats and work-item steps alike: the messages, then trace lines with each prompt, run and tool call",
            ),
            Place::new("logs", self.root.logs(), "the server log, one JSON line per event"),
            Place::new("code-map", self.root.code_map(), "the code maps of the projects worked on"),
        ];
        if let Some(database) = self.root.database() {
            places.push(Place::new(
                "database",
                database,
                "SQLite: the session list (titles, folders, dates), memories, applications, work items and their events, connections and preferences",
            ));
        }
        if let Some(memory) = self.root.memory() {
            places.push(Place::new("memory", memory, "memory kept before it moved into the database"));
        }
        if let Some(own) = own_folder() {
            places.push(Place::new("own", own, "the user's settings, skills, agents, tools and content overrides"));
        }
        for (name, data, _) in self.root.former() {
            places.push(Place::new(&format!("former:{name}"), data, "data kept while the app had a former name"));
        }
        places
    }

    fn sessions(&self, query: &Query) -> Vec<Session> {
        self.shelf().sessions(query)
    }

    fn search(&self, query: &Query) -> Vec<Hit> {
        self.shelf().search(query)
    }

    fn read(&self, session: &str, from: usize, count: usize) -> Result<Read, String> {
        self.shelf().read(session, from, count)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use domain::SessionId;

    use super::*;
    use crate::catalog::InMemorySessionCatalog;

    #[test]
    fn a_session_found_on_disk_carries_the_title_the_catalog_gave_it() {
        let data = tempfile::tempdir().expect("data");
        let root = DataRoot::under(data.path());
        let catalog: Arc<dyn SessionCatalog> = Arc::new(InMemorySessionCatalog::default());
        let (cwd, id) = (Path::new("/work/gateway"), SessionId("s1".into()));
        catalog.update(&mut |kept| kept.started(&id, cwd, 1));
        catalog.update(&mut |kept| kept.rename(&id, "Gateway em Go"));
        std::fs::create_dir_all(root.sessions()).expect("dir");
        let line = serde_json::json!({"kind": "prompted", "at_ms": 3, "prompt": "uma gateway em Go", "seq": 1});
        std::fs::write(root.sessions().join(format!("{}.jsonl", scoped_id(cwd, &id).0)), format!("{line}\n"))
            .expect("write");
        let history = KeptHistory::new(root, catalog);
        let hits = history.search(&Query { text: "gateway".into(), limit: 5, ..Query::default() });
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].session.title.as_deref(), Some("Gateway em Go"));
        assert_eq!(hits[0].session.cwd.as_deref(), Some("/work/gateway"));
        assert_eq!(hits[0].session.kept_in, CURRENT);
        let places = history.places();
        assert!(places.iter().any(|place| place.name == "sessions" && place.exists));
        assert!(places.iter().all(|place| !place.name.starts_with("former:")), "a folder of its own has no past");
    }
}
