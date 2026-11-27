use domain::connection::{Capability, Connection, ConnectionId, ProviderKey};
use serde_json::Value;
use store::{ConnectionRecord, Store};

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub connection: Connection,
    pub settings: Value,
    pub owner: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Book {
    pub entries: Vec<Entry>,
}

fn entry(record: ConnectionRecord, owner: String) -> Entry {
    Entry {
        owner,
        connection: Connection {
            id: ConnectionId::new(record.id),
            provider: ProviderKey::new(record.provider),
            label: record.label,
            account: record.account.unwrap_or_default(),
            capabilities: record.capabilities.into_iter().map(Capability::new).collect(),
        },
        settings: record.settings,
    }
}

impl Book {
    pub fn load(store: &Store) -> Result<Book, store::Error> {
        let mut entries = Vec::new();
        for record in store.connections()? {
            let owner = store.owner_of(store::ownership::CONNECTION, &record.id)?;
            entries.push(entry(record, owner));
        }
        Ok(Book { entries })
    }

    pub fn find(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.connection.id.as_str() == id)
    }

    pub fn default_for(&self, capability: &str) -> Option<&Entry> {
        let capability = Capability::from(capability);
        self.entries.iter().find(|entry| entry.connection.offers(&capability))
    }

    pub fn resolve(&self, connection: Option<&str>, capability: &str) -> Option<&Entry> {
        match connection {
            Some(id) => self.find(id),
            None => self.default_for(capability),
        }
    }

    pub fn resolve_for(&self, owner: &str, connection: Option<&str>, capability: &str) -> Option<&Entry> {
        let capability = Capability::from(capability);
        let mut theirs = self.entries.iter().filter(|entry| entry.owner == owner);
        match connection {
            Some(id) => theirs.find(|entry| entry.connection.id.as_str() == id),
            None => theirs.find(|entry| entry.connection.offers(&capability)),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn record(id: &str, capabilities: &[&str]) -> ConnectionRecord {
        ConnectionRecord {
            id: id.into(),
            provider: "p".into(),
            label: id.into(),
            account: None,
            capabilities: capabilities.iter().map(|c| (*c).to_owned()).collect(),
            settings: json!({"project": id}),
        }
    }

    #[test]
    fn an_explicit_connection_wins_and_otherwise_the_first_offering_the_capability() {
        let Ok(store) = Store::in_memory(0) else { panic!("store") };
        for record in [record("a", &["code"]), record("b", &["tracker"]), record("c", &["tracker"])] {
            assert!(store.save_connection(&record).is_ok());
        }
        let Ok(book) = Book::load(&store) else { panic!("book") };
        assert_eq!(book.resolve(None, "tracker").map(|e| e.connection.id.as_str()), Some("b"));
        assert_eq!(book.resolve(Some("c"), "tracker").map(|e| e.settings["project"].clone()), Some(json!("c")));
        assert!(book.resolve(Some("z"), "tracker").is_none());
        assert!(book.resolve(None, "pipelines").is_none());
    }
}

#[test]
fn each_owner_resolves_only_to_their_connections() {
    let entry = |id: &str, owner: &str| Entry {
        connection: Connection {
            id: ConnectionId::new(id.to_owned()),
            provider: ProviderKey::new("p".to_owned()),
            label: id.to_owned(),
            account: String::new(),
            capabilities: vec![Capability::new("tracker".to_owned())],
        },
        settings: Value::Null,
        owner: owner.to_owned(),
    };
    let book = Book { entries: vec![entry("a", "u-ana"), entry("b", "u-bia")] };
    let found = |owner: &str, id: Option<&str>| {
        book.resolve_for(owner, id, "tracker").map(|e| e.connection.id.as_str().to_owned())
    };
    assert_eq!(found("u-bia", None), Some("b".to_owned()));
    assert_eq!(found("u-bia", Some("a")), None);
    assert_eq!(found("u-ana", Some("a")), Some("a".to_owned()));
}
