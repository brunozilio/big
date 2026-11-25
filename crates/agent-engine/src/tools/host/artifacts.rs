use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use domain::tools::artifact::{self, Artifact, Kind};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{Reach, text};

pub const SPACE: &str = "artifacts";

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "publish" => publish(reach, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn shelf(reach: &Reach<'_>) -> Result<PathBuf, String> {
    let kept = reach.workspace.services.artifacts.clone();
    let found =
        kept.or_else(|| platform::data_dir(brand::NAME, brand::COMMAND).map(|data| data.join(artifact::FOLDER)));
    found.ok_or_else(|| "this user has no data folder to keep artifacts in".to_owned())
}

fn id_for(session: &str, source: &Path) -> String {
    let digest =
        Sha256::new().chain_update(session).chain_update([0]).chain_update(source.as_os_str().as_encoded_bytes());
    digest.finalize().iter().take(12).map(|byte| format!("{byte:02x}")).collect()
}

fn safe_name(source: &Path) -> String {
    let base = source.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|letter| if letter.is_alphanumeric() || ".-_ ".contains(letter) { letter } else { '_' })
        .collect();
    if cleaned.trim_matches('.').is_empty() { "artifact".to_owned() } else { cleaned }
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
}

fn checked(source: &Path) -> Result<(Kind, Vec<u8>), String> {
    let name = safe_name(source);
    let kind = artifact::kind_of(&name).ok_or_else(|| {
        format!("{name} is not a kind of file an artifact shows; publish one of {}", artifact::accepted())
    })?;
    let size = std::fs::metadata(source).map_err(|error| format!("{}: {error}", source.display()))?.len();
    if size > artifact::MAX_BYTES {
        return Err(format!("{name} has {size} bytes, more than the {} an artifact takes", artifact::MAX_BYTES));
    }
    let bytes = std::fs::read(source).map_err(|error| error.to_string())?;
    let textual = matches!(kind, Kind::Page | Kind::Markdown | Kind::Text);
    if textual && std::str::from_utf8(&bytes).is_err() {
        return Err(format!("{name} is not UTF-8 text"));
    }
    Ok((kind, bytes))
}

fn store(folder: &Path, published: &Artifact, bytes: &[u8]) -> Result<(), String> {
    let failed = |error: std::io::Error| format!("the artifact was not kept: {error}");
    if folder.exists() {
        std::fs::remove_dir_all(folder).map_err(failed)?;
    }
    std::fs::create_dir_all(folder).map_err(failed)?;
    std::fs::write(folder.join(&published.name), bytes).map_err(failed)?;
    let meta = serde_json::to_vec_pretty(published).map_err(|error| error.to_string())?;
    std::fs::write(folder.join(artifact::META), meta).map_err(failed)
}

fn publish(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let place = reach.workspace.place(text(arguments, "path")?);
    reach.fence.reads(&place.real)?;
    let (kind, bytes) = checked(&place.real)?;
    let name = safe_name(&place.real);
    let given = arguments.get("title").and_then(Value::as_str);
    let title = artifact::title_of(given, kind, &name, &String::from_utf8_lossy(&bytes));
    let session = reach.workspace.session_id.clone();
    let published = Artifact {
        id: id_for(&session, &place.real),
        title,
        name,
        kind,
        session,
        source: place.real.display().to_string(),
        published_at: now(),
        size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
    };
    store(&shelf(reach)?.join(&published.id), &published, &bytes)?;
    Ok(json!({ "id": published.id, "title": published.title, "kind": published.kind, "name": published.name }))
}

#[cfg(test)]
mod tests {
    use super::super::Fence;
    use super::*;
    use crate::tools::{Effect, Workspace};

    fn workspace(cwd: &Path, shelf: &Path) -> Workspace {
        let mut workspace = Workspace::new(cwd);
        workspace.session_id = "s-1".into();
        workspace.services.artifacts = Some(shelf.to_path_buf());
        workspace
    }

    #[test]
    fn a_published_file_is_kept_with_its_title_and_publishing_it_again_replaces_it() {
        let cwd = tempfile::tempdir().expect("cwd");
        let shelf = tempfile::tempdir().expect("shelf");
        let page = cwd.path().join("q3.html");
        std::fs::write(&page, "<title>Vendas Q3</title><p>1</p>").expect("page");
        let workspace = workspace(cwd.path(), shelf.path());
        let effects = [Effect::ReadPath(workspace.place("q3.html"))];
        let reach = Reach { workspace: &workspace, fence: Fence::of(&effects), streams: None };

        let first = call(&reach, "publish", &json!({ "path": "q3.html" })).expect("published");
        std::fs::write(&page, "<p>2</p>").expect("page");
        let again = call(&reach, "publish", &json!({ "path": "q3.html", "title": "Q3" })).expect("published");

        assert_eq!(first["title"], "Vendas Q3");
        assert_eq!(
            (again["id"].clone(), again["title"].clone(), again["kind"].clone()),
            (first["id"].clone(), json!("Q3"), json!("page"))
        );
        let folder = shelf.path().join(first["id"].as_str().unwrap_or_default());
        assert_eq!(std::fs::read_to_string(folder.join("q3.html")).expect("kept"), "<p>2</p>");
        let meta: Artifact =
            serde_json::from_slice(&std::fs::read(folder.join(artifact::META)).expect("meta")).expect("json");
        assert_eq!((meta.session.as_str(), meta.size, meta.kind), ("s-1", 8, Kind::Page));
    }

    #[test]
    fn only_known_kinds_inside_the_fence_are_published() {
        let cwd = tempfile::tempdir().expect("cwd");
        let shelf = tempfile::tempdir().expect("shelf");
        std::fs::write(cwd.path().join("run.sh"), "echo").expect("file");
        std::fs::write(cwd.path().join("a.html"), "<p>").expect("file");
        let workspace = workspace(cwd.path(), shelf.path());
        let effects = [Effect::ReadPath(workspace.place("run.sh"))];
        let reach = Reach { workspace: &workspace, fence: Fence::of(&effects), streams: None };

        let refused = call(&reach, "publish", &json!({ "path": "run.sh" })).expect_err("unknown kind");
        let fenced = call(&reach, "publish", &json!({ "path": "a.html" }));

        assert!(refused.contains("not a kind of file"), "{refused}");
        assert!(fenced.is_err());
        assert_eq!(std::fs::read_dir(shelf.path()).expect("shelf").count(), 0);
    }
}
