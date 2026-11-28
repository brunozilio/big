pub(crate) mod legacy;
mod sqlite;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Mutex;

use platform::sync::Locked;

use domain::MemoryBook;

pub use sqlite::SqliteMemoryStore;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Revised {
    pub revision: u64,
    pub book: MemoryBook,
}

pub trait MemoryStore: Send + Sync {
    fn revision(&self, key: &str) -> u64;
    fn read(&self, key: &str) -> Revised;
    fn update(&self, key: &str, change: &mut (dyn FnMut(&mut MemoryBook) + Send)) -> std::io::Result<Revised>;
    fn undo_point(&self, key: &str) -> Option<MemoryBook>;
    fn set_undo_point(&self, key: &str, book: Option<&MemoryBook>) -> std::io::Result<()>;

    fn load(&self, key: &str) -> MemoryBook {
        self.read(key).book
    }

    fn save(&self, key: &str, book: &MemoryBook) -> std::io::Result<()> {
        self.update(key, &mut |kept| *kept = book.clone()).map(drop)
    }
}

#[derive(Debug, Default)]
pub struct InMemoryStore {
    books: Mutex<HashMap<String, Revised>>,
    undo_points: Mutex<HashMap<String, MemoryBook>>,
}

impl MemoryStore for InMemoryStore {
    fn revision(&self, key: &str) -> u64 {
        self.books.locked().get(key).map_or(0, |kept| kept.revision)
    }

    fn read(&self, key: &str) -> Revised {
        self.books.locked().get(key).cloned().unwrap_or_default()
    }

    fn update(&self, key: &str, change: &mut (dyn FnMut(&mut MemoryBook) + Send)) -> std::io::Result<Revised> {
        let mut books = self.books.locked();
        let kept = books.entry(key.to_owned()).or_default();
        let before = kept.book.clone();
        change(&mut kept.book);
        if kept.book != before {
            kept.revision += 1;
        }
        Ok(kept.clone())
    }

    fn undo_point(&self, key: &str) -> Option<MemoryBook> {
        self.undo_points.locked().get(key).cloned()
    }

    fn set_undo_point(&self, key: &str, book: Option<&MemoryBook>) -> std::io::Result<()> {
        let mut points = self.undo_points.locked();
        match book {
            Some(book) => points.insert(key.to_owned(), book.clone()),
            None => points.remove(key),
        };
        Ok(())
    }
}
