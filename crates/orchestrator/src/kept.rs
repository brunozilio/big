use std::path::{Path, PathBuf};

use base64::Engine as _;
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

const MOST_SENT: u64 = 20 * 1024 * 1024;

pub const FILE: &str = "$file";

fn media_type(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()).unwrap_or_default();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "txt" | "log" => "text/plain",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

fn kept_file(store: &Path, id: &str) -> Option<PathBuf> {
    let valid = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric());
    let folder = store.join(id);
    let found = std::fs::read_dir(folder).ok().filter(|_| valid)?.filter_map(Result::ok).next()?;
    Some(found.path())
}

fn loaded(store: &Path, id: &str) -> Value {
    let Some(file) = kept_file(store, id) else { return json!({ "id": id, "missing": true }) };
    let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let size = std::fs::metadata(&file).map(|meta| meta.len()).unwrap_or_default();
    let mut shown = json!({ "id": id, "name": name, "size": size, "contentType": media_type(&name) });
    if size > MOST_SENT {
        shown["skipped"] = json!("too large");
        return shown;
    }
    match std::fs::read(&file) {
        Ok(bytes) => shown["base64"] = json!(base64::engine::general_purpose::STANDARD.encode(bytes)),
        Err(_) => shown["missing"] = json!(true),
    }
    shown
}

pub fn expand(store: &Path, value: &mut Value) {
    match value {
        Value::Object(fields) => {
            if fields.len() == 1
                && let Some(id) = fields.get(FILE).and_then(Value::as_str).map(str::to_owned)
            {
                *value = loaded(store, &id);
                return;
            }
            fields.values_mut().for_each(|inner| expand(store, inner));
        }
        Value::Array(items) => items.iter_mut().for_each(|inner| expand(store, inner)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_reference_in_the_arguments_becomes_its_content_and_a_foreign_path_does_not() {
        let root = tempfile::tempdir().expect("root");
        let tree = root.path().join("tree");
        std::fs::create_dir_all(&tree).expect("tree");
        std::fs::write(tree.join("a.png"), "png").expect("shot");
        let store = store(&root.path().join("data"));
        let kept = keep(&store, &tree, &json!(["a.png"]));
        let id = kept[0]["id"].as_str().expect("id").to_owned();
        let mut args = json!({ "issue": "T-1", "files": [{ "$file": id }, { "$file": "../tree" }, { "$file": "x", "more": 1 }] });
        expand(&store, &mut args);
        assert_eq!(args["files"][0], json!({ "id": id, "name": "a.png", "size": 3, "contentType": "image/png", "base64": "cG5n" }));
        assert_eq!(args["files"][1]["missing"], true);
        assert_eq!(args["files"][2], json!({ "$file": "x", "more": 1 }));
        assert_eq!(args["issue"], "T-1");
    }

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
