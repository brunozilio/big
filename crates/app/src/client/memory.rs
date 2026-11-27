use std::path::Path;

use domain::{Author, Memory, MemoryEdit, MemoryId, Provenance};

use crate::Client;
use crate::api::memory::MemoryListing;
use crate::features::memory::{self, MemoryShelf};

impl Client {
    pub fn memory_shelf(&self, shelf: &MemoryShelf) -> MemoryListing {
        memory::of(&self.services).shelf(shelf)
    }

    pub fn team_memories(&self, project: &Path) -> Vec<Memory> {
        memory::of(&self.services).shared(project)
    }

    pub fn edit_memory(&self, shelf: &MemoryShelf, edit: MemoryEdit) -> Result<MemoryListing, String> {
        let by = Provenance { author: Author::User, now: memory::now(), origin: None };
        memory::of(&self.services).edit_shelf(shelf, &[edit], by)
    }

    pub fn move_memory(&self, from: &MemoryShelf, id: MemoryId, to: &MemoryShelf) -> Result<(), String> {
        memory::of(&self.services).move_memory(from, id, to)
    }

    pub fn memory_mcp(&self, project: &Path, origin: &str) -> domain::mcp::LocalMcp {
        let memories = memory::of(&self.services);
        memory::mcp::MemoryEndpoint::for_agent(memories, project.to_path_buf(), origin.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use agent_engine::fake::FakeEngine;
    use domain::{MemoryEdit, MemoryKind, MemoryScope};

    use crate::features::memory::{ApplicationRef, MemoryShelf};
    use crate::{Client, LocalServices};

    fn client() -> Client {
        let rocket = |checkout: &Path| {
            checkout.starts_with("/work/rocket").then(|| ApplicationRef { id: "a1".into(), name: "Rocket".into() })
        };
        let services = LocalServices::with_engine(Arc::new(FakeEngine::new(Vec::new(), Vec::new())))
            .with_applications(Arc::new(rocket));
        Client::builder().services(services).build().expect("client")
    }

    fn remember(text: &str, scope: MemoryScope) -> MemoryEdit {
        MemoryEdit::Remember {
            kind: MemoryKind::Semantic,
            text: text.into(),
            tags: Vec::new(),
            touches: Vec::new(),
            scope,
        }
    }

    #[test]
    fn a_memory_forgotten_from_the_screen_can_be_brought_back() {
        let client = client();
        let shelf = MemoryShelf::Repository(PathBuf::from("/work/rocket/api"));
        let listed =
            client.edit_memory(&shelf, remember("the build uses bun", MemoryScope::Repository)).expect("remember");
        let id = listed.memories[0].id;
        assert_eq!(listed.memories[0].author, domain::Author::User);
        let forgotten = client.edit_memory(&shelf, MemoryEdit::Forget { id }).expect("forget");
        assert_eq!((forgotten.memories.len(), forgotten.forgotten.len()), (0, 1));
        let restored = client.edit_memory(&shelf, MemoryEdit::Restore { id }).expect("restore");
        assert_eq!(client.memory_shelf(&shelf), restored);
        let other = MemoryShelf::Repository(PathBuf::from("/work/shop"));
        assert!(client.memory_shelf(&other).memories.is_empty(), "each repository keeps its own");
    }

    #[test]
    fn a_memory_moved_to_a_wider_shelf_leaves_the_narrower_one() {
        let client = client();
        let api = MemoryShelf::Repository(PathBuf::from("/work/rocket/api"));
        let app = MemoryShelf::Application("a1".into());
        let id =
            client.edit_memory(&api, remember("deploys on tuesdays", MemoryScope::Repository)).expect("kept").memories
                [0]
            .id;
        client.move_memory(&api, id, &app).expect("moved");
        assert!(client.memory_shelf(&api).memories.is_empty());
        assert!(client.memory_shelf(&api).forgotten.is_empty(), "moved, not forgotten");
        assert_eq!(client.memory_shelf(&app).memories[0].text, "deploys on tuesdays");
        assert!(client.move_memory(&api, id, &app).is_err(), "nothing left to move");
    }

    #[test]
    fn a_run_reads_its_repository_its_application_and_the_global_memory() {
        let client = client();
        let memories = crate::features::memory::of(&client.services);
        let by = || domain::Provenance { author: domain::Author::Agent, now: 1, origin: None };
        let edits = [
            remember("api uses axum", MemoryScope::Repository),
            remember("deploys on tuesdays", MemoryScope::Application),
            remember("answers in portuguese", MemoryScope::Global),
        ];
        let listed = memories.edit(Path::new("/work/rocket/api"), &edits, by()).expect("kept");
        assert_eq!(listed.memories.len(), 3);
        let scopes: Vec<MemoryScope> = listed.memories.iter().map(|m| MemoryScope::of(m.id).0).collect();
        assert_eq!(scopes, MemoryScope::ALL.to_vec());
        let web = memories.load(Path::new("/work/rocket/web")).memories;
        assert_eq!(
            web.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
            ["deploys on tuesdays", "answers in portuguese"]
        );
        let shop = memories.load(Path::new("/work/shop")).memories;
        assert_eq!(shop.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["answers in portuguese"]);
        let alone = memories.edit(Path::new("/work/shop"), &[remember("uses pnpm", MemoryScope::Application)], by());
        let alone = alone.expect("kept");
        assert!(alone.memories.iter().any(|m| m.text == "uses pnpm" && MemoryScope::of(m.id).0.is_repository()));
        let global = listed.memories.iter().find(|m| m.text == "answers in portuguese").map(|m| m.id).expect("global");
        memories.edit(Path::new("/work/shop"), &[MemoryEdit::Forget { id: global }], by()).expect("forget");
        assert!(
            memories.load(Path::new("/work/rocket/api")).memories.iter().all(|m| m.text != "answers in portuguese")
        );
    }
}
