use std::path::{Path, PathBuf};

use serde_json::{Value, json};

const MOST_BYTES: u64 = 256 * 1024 * 1024;

pub fn store(data: &Path) -> PathBuf {
    data.join("attachments")
}

fn inside(worktree: &Path, wanted: &str) -> Option<PathBuf> {
    let root = worktree.canonicalize().ok()?;
    let found = root.join(wanted).canonicalize().ok()?;
    let fits = std::fs::metadata(&found).is_ok_and(|meta| meta.is_file() && meta.len() <= MOST_BYTES);
    (found.starts_with(&root) && fits).then_some(found)
}

fn copied(store: &Path, file: &Path, wanted: &str) -> std::io::Result<Value> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let name = file.file_name().map_or_else(|| "evidencia".to_owned(), |name| name.to_string_lossy().into_owned());
    let folder = store.join(&id);
    std::fs::create_dir_all(&folder)?;
    let size = std::fs::copy(file, folder.join(&name))?;
    Ok(json!({ "id": id, "name": name, "size": size, "path": wanted }))
}

pub fn keep(store: &Path, worktree: &Path, wanted: &Value) -> Vec<Value> {
    let listed = wanted.as_array().map(Vec::as_slice).unwrap_or_default();
    listed
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|path| copied(store, &inside(worktree, path)?, path).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_inside_the_worktree_are_copied_and_the_rest_is_skipped() {
        let root = tempfile::tempdir().expect("root");
        let tree = root.path().join("tree");
        std::fs::create_dir_all(tree.join("shots")).expect("tree");
        std::fs::write(tree.join("shots/a.png"), "png").expect("shot");
        std::fs::write(root.path().join("secret.txt"), "x").expect("outside");
        let store = store(&root.path().join("data"));
        let wanted = json!(["shots/a.png", "../secret.txt", "missing.webm", "shots"]);
        let kept = keep(&store, &tree, &wanted);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0]["name"], "a.png");
        assert_eq!(kept[0]["path"], "shots/a.png");
        let id = kept[0]["id"].as_str().expect("id");
        assert_eq!(std::fs::read_to_string(store.join(id).join("a.png")).expect("copy"), "png");
    }
}
