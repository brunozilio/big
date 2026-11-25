use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use axum::extract::{Path as Segment, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use domain::tools::artifact::{self, Artifact, Kind};

use crate::error::{ApiError, ApiResult, json as reply, said};
use crate::state::Ctx;

const SANDBOX: &str = "sandbox allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox; \
default-src 'none'; \
script-src 'unsafe-inline' 'unsafe-eval' https://cdnjs.cloudflare.com https://cdn.jsdelivr.net/npm/ https://unpkg.com https://cdn.tailwindcss.com https://code.jquery.com; \
style-src 'unsafe-inline' https://fonts.googleapis.com; \
font-src https://fonts.gstatic.com data:; \
img-src data: blob:; media-src data: blob:; worker-src blob:; \
form-action 'none'; base-uri 'none'";

fn shelf(ctx: &Ctx) -> PathBuf {
    ctx.backend.data.join(artifact::FOLDER)
}

fn missing() -> ApiError {
    ApiError::missing(said("There is no such artifact.", "No existe ese artefacto.", "Não há esse artefato."))
}

fn found(shelf: &Path, id: &str) -> ApiResult<(Artifact, PathBuf)> {
    if !artifact::valid_id(id) {
        return Err(missing());
    }
    let folder = shelf.join(id);
    let meta = std::fs::read(folder.join(artifact::META)).map_err(|_| missing())?;
    let kept: Artifact = serde_json::from_slice(&meta).map_err(ApiError::failed)?;
    let file = folder.join(Path::new(&kept.name).file_name().ok_or_else(missing)?);
    Ok((kept, file))
}

pub async fn meta(State(ctx): State<Ctx>, Segment(id): Segment<String>) -> ApiResult<Response> {
    let (kept, _) = found(&shelf(&ctx), &id)?;
    Ok(reply(serde_json::to_value(kept).map_err(ApiError::failed)?))
}

fn encoded(name: &str) -> String {
    name.bytes().fold(String::new(), |mut out, byte| {
        if byte.is_ascii_alphanumeric() || b".-_".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
        out
    })
}

fn disposition(name: &str, download: bool) -> String {
    let plain: String = name
        .chars()
        .map(|letter| if letter.is_ascii_alphanumeric() || ".-_ ".contains(letter) { letter } else { '_' })
        .collect();
    let how = if download { "attachment" } else { "inline" };
    format!("{how}; filename=\"{plain}\"; filename*=UTF-8''{}", encoded(name))
}

fn body(kept: &Artifact, bytes: Vec<u8>) -> Vec<u8> {
    match kept.kind {
        Kind::Page => artifact::wrapped(&String::from_utf8_lossy(&bytes)).into_bytes(),
        _ => bytes,
    }
}

fn policy(kept: &Artifact) -> Option<&'static str> {
    match kept.kind {
        Kind::Pdf => None,
        _ => Some(SANDBOX),
    }
}

pub async fn content(
    State(ctx): State<Ctx>,
    Segment(id): Segment<String>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let (kept, file) = found(&shelf(&ctx), &id)?;
    let bytes = tokio::fs::read(&file).await.map_err(|_| missing())?;
    let download = query.get("download").is_some_and(|value| value == "1");
    let mut response = (
        [
            (header::CONTENT_TYPE, artifact::media_type(&kept.name).to_owned()),
            (header::CONTENT_DISPOSITION, disposition(&kept.name, download)),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        body(&kept, bytes),
    )
        .into_response();
    if let Some(rule) = policy(&kept) {
        response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, header::HeaderValue::from_static(rule));
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(kind: Kind, name: &str) -> Artifact {
        Artifact {
            id: "ab".into(),
            title: "T".into(),
            name: name.into(),
            kind,
            session: String::new(),
            source: String::new(),
            published_at: 0,
            size: 0,
        }
    }

    #[test]
    fn an_artifact_is_found_only_by_a_hex_id_with_its_meta() {
        let shelf = tempfile::tempdir().expect("shelf");
        let folder = shelf.path().join("ab");
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(folder.join(artifact::META), serde_json::to_vec(&kept(Kind::Page, "q.html")).expect("json"))
            .expect("meta");

        let (found_meta, file) = found(shelf.path(), "ab").expect("found");

        assert_eq!((found_meta.title.as_str(), file), ("T", folder.join("q.html")));
        assert!(found(shelf.path(), "../ab").is_err());
        assert!(found(shelf.path(), "cd").is_err());
    }

    #[test]
    fn pages_run_sandboxed_and_names_survive_the_download_header() {
        assert_eq!(policy(&kept(Kind::Page, "a.html")), Some(SANDBOX));
        assert_eq!(policy(&kept(Kind::Image, "a.svg")), Some(SANDBOX));
        assert_eq!(policy(&kept(Kind::Pdf, "a.pdf")), None);
        assert!(SANDBOX.starts_with("sandbox ") && !SANDBOX.contains("allow-same-origin"));
        assert_eq!(
            disposition("relatório v2.html", true),
            "attachment; filename=\"relat_rio v2.html\"; filename*=UTF-8''relat%C3%B3rio%20v2.html"
        );
        let page = body(&kept(Kind::Page, "a.html"), b"<p>x</p>".to_vec());
        assert!(String::from_utf8_lossy(&page).starts_with("<!doctype html>"));
        assert_eq!(body(&kept(Kind::Text, "a.csv"), b"a,b".to_vec()), b"a,b");
    }
}
