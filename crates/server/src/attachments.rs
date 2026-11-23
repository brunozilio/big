use std::path::{Path, PathBuf};

use axum::extract::{Multipart, State};
use axum::response::Response;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply};
use crate::state::Ctx;

pub fn random_id() -> ApiResult<String> {
    let mut bytes = [0_u8; 12];
    getrandom::fill(&mut bytes).map_err(ApiError::failed)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn safe_name(name: &str) -> String {
    let base = Path::new(name).file_name().map(|file| file.to_string_lossy().into_owned()).unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|letter| if letter.is_alphanumeric() || ".-_ ".contains(letter) { letter } else { '_' })
        .collect();
    if cleaned.trim_matches('.').is_empty() { "anexo".to_owned() } else { cleaned }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|letter| letter.is_ascii_hexdigit())
}

pub fn paths(dir: &Path, ids: &Value) -> Vec<PathBuf> {
    let listed = ids.as_array().map(Vec::as_slice).unwrap_or_default();
    listed
        .iter()
        .filter_map(Value::as_str)
        .filter(|id| valid_id(id))
        .filter_map(|id| std::fs::read_dir(dir.join(id)).ok()?.flatten().next().map(|entry| entry.path()))
        .collect()
}

pub async fn upload(State(ctx): State<Ctx>, mut form: Multipart) -> ApiResult<Response> {
    while let Some(field) = form.next_field().await.map_err(|error| ApiError::bad(error.to_string()))? {
        if field.name() != Some("file") {
            continue;
        }
        let name = safe_name(field.file_name().unwrap_or("anexo"));
        let bytes = field.bytes().await.map_err(|error| ApiError::bad(error.to_string()))?;
        let id = random_id()?;
        let folder = ctx.backend.attachments().join(&id);
        std::fs::create_dir_all(&folder).map_err(ApiError::failed)?;
        std::fs::write(folder.join(&name), &bytes).map_err(ApiError::failed)?;
        return Ok(reply(json!({ "id": id, "name": name, "size": bytes.len() })));
    }
    Err(ApiError::bad("Envie o arquivo no campo file."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_stay_inside_their_folder_and_ids_are_hex() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name(".."), "anexo");
        assert!(valid_id("ab12") && !valid_id("../x"));
        let dir = tempfile::tempdir().expect("dir");
        std::fs::create_dir_all(dir.path().join("ab")).expect("folder");
        std::fs::write(dir.path().join("ab/a.txt"), "x").expect("file");
        assert_eq!(paths(dir.path(), &json!(["ab", "zz", "../x"])), vec![dir.path().join("ab/a.txt")]);
    }
}
