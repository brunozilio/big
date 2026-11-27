use std::path::{Path, PathBuf};

use axum::extract::{Multipart, Path as Segment, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply, said};
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

pub async fn upload(
    State(ctx): State<Ctx>,
    axum::Extension(person): axum::Extension<domain::principal::Principal>,
    mut form: Multipart,
) -> ApiResult<Response> {
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
        ctx.backend.store.set_owner(store::ownership::ATTACHMENT, &id, person.owner())?;
        return Ok(reply(json!({ "id": id, "name": name, "size": bytes.len() })));
    }
    Err(ApiError::bad(said(
        "Send the file in the file field.",
        "Envía el archivo en el campo file.",
        "Envie o arquivo no campo file.",
    )))
}

const KINDS: [(&str, &str); 12] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("webm", "video/webm"),
    ("mp4", "video/mp4"),
    ("txt", "text/plain; charset=utf-8"),
    ("log", "text/plain; charset=utf-8"),
    ("json", "application/json"),
    ("html", "text/plain; charset=utf-8"),
    ("zip", "application/zip"),
];

pub fn kind_of(name: &str) -> &'static str {
    let extension = Path::new(name).extension().map(|found| found.to_string_lossy().to_lowercase()).unwrap_or_default();
    KINDS.iter().find(|(known, _)| *known == extension).map_or("application/octet-stream", |(_, kind)| kind)
}

pub async fn download(State(ctx): State<Ctx>, Segment(id): Segment<String>) -> ApiResult<Response> {
    let file = paths(&ctx.backend.attachments(), &json!([id])).into_iter().next();
    let file = file.ok_or_else(|| {
        ApiError::missing(said("There is no such attachment.", "No existe ese adjunto.", "Não há esse anexo."))
    })?;
    let name = file.file_name().map(|found| found.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = tokio::fs::read(&file).await.map_err(ApiError::failed)?;
    let disposition = format!("inline; filename=\"{}\"", safe_name(&name));
    Ok(([(header::CONTENT_TYPE, kind_of(&name).to_owned()), (header::CONTENT_DISPOSITION, disposition)], bytes)
        .into_response())
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

    #[test]
    fn evidence_is_served_with_its_kind_and_html_never_renders() {
        assert_eq!(kind_of("shot.PNG"), "image/png");
        assert_eq!(kind_of("run.webm"), "video/webm");
        assert_eq!(kind_of("page.html"), "text/plain; charset=utf-8");
        assert_eq!(kind_of("trace"), "application/octet-stream");
    }
}
