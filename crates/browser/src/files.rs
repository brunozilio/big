use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

pub fn attach(input: &Value, roots: &[PathBuf], limit: u64) -> Result<Value, String> {
    let paths: Vec<&str> =
        input["paths"].as_array().map(|paths| paths.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if paths.is_empty() {
        return Err("Give `paths`: one or more absolute paths of files to upload.".to_owned());
    }
    let allowed: Vec<PathBuf> = roots.iter().filter_map(|root| root.canonicalize().ok()).collect();
    let mut total = 0;
    let mut files = Vec::new();
    for path in paths {
        let file = shared(Path::new(path), &allowed)?;
        let bytes = std::fs::read(&file).map_err(|error| format!("Could not read {path}: {error}"))?;
        total += bytes.len() as u64;
        if total > limit {
            return Err(format!(
                "The files add up to more than {} MB; upload fewer or smaller files per call.",
                limit / (1024 * 1024)
            ));
        }
        let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        files.push(json!({ "name": name, "mimeType": mime(&name), "data": STANDARD.encode(bytes) }));
    }
    let mut prepared = input.clone();
    prepared["files"] = json!(files);
    Ok(prepared)
}

fn shared(path: &Path, allowed: &[PathBuf]) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("{} is not an absolute path.", path.display()));
    }
    let real = path.canonicalize().map_err(|_| format!("{} does not exist.", path.display()))?;
    if !real.is_file() {
        return Err(format!("{} is not a file.", path.display()));
    }
    if !allowed.iter().any(|root| real.starts_with(root)) {
        return Err(format!(
            "{} is not in a folder shared with this session; only files under the session's folders can be uploaded.",
            path.display()
        ));
    }
    Ok(real)
}

pub fn mime(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()).unwrap_or_default();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "zip" => "application/zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

pub fn stamp() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis()).unwrap_or_default()
}

pub fn keep(folder: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|error| format!("Could not create {}: {error}", folder.display()))?;
    let path = folder.join(name);
    std::fs::write(&path, bytes).map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    Ok(path)
}

pub fn save_images(mut answer: Value, folder: &Path) -> Value {
    let Some(parts) = answer["content"].as_array_mut() else { return answer };
    let mut saved = Vec::new();
    for part in parts.iter().filter(|part| part["type"] == "image") {
        let data = part["data"].as_str().unwrap_or_default();
        let extension = if part["mimeType"] == "image/png" { "png" } else { "jpg" };
        let kept = STANDARD
            .decode(data)
            .map_err(|error| error.to_string())
            .and_then(|bytes| keep(folder, &format!("screenshot-{}.{extension}", stamp()), &bytes));
        saved.push(match kept {
            Ok(path) => format!("Saved to: {}", path.display()),
            Err(error) => format!("Could not save the image: {error}"),
        });
    }
    if !saved.is_empty() {
        parts.push(json!({ "type": "text", "text": saved.join("\n") }));
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_files_under_a_shared_folder_are_read() {
        let shared_dir = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let other = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let inside = shared_dir.path().join("a.png");
        let outside = other.path().join("b.txt");
        std::fs::write(&inside, b"png").unwrap_or_default();
        std::fs::write(&outside, b"txt").unwrap_or_default();
        let roots = vec![shared_dir.path().to_path_buf()];
        let input = json!({ "paths": [inside.display().to_string()], "ref": "ref_1", "tabId": 1 });
        let prepared = attach(&input, &roots, 1024).unwrap_or_default();
        assert_eq!(prepared["files"][0]["name"], "a.png");
        assert_eq!(prepared["files"][0]["mimeType"], "image/png");
        assert_eq!(prepared["files"][0]["data"], STANDARD.encode(b"png"));
        let refused = attach(&json!({ "paths": [outside.display().to_string()] }), &roots, 1024);
        assert!(refused.is_err_and(|error| error.contains("not in a folder shared")));
        assert!(attach(&json!({ "paths": ["relative.txt"] }), &roots, 1024).is_err());
        assert!(attach(&input, &roots, 2).is_err_and(|error| error.contains("MB")));
        assert!(attach(&json!({ "paths": [] }), &roots, 1024).is_err());
    }

    #[test]
    fn images_asked_to_be_saved_land_in_the_folder_and_are_named_in_the_answer() {
        let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let answer = json!({ "content": [
            { "type": "text", "text": "Successfully captured screenshot" },
            { "type": "image", "data": STANDARD.encode(b"jpeg"), "mimeType": "image/jpeg" }
        ]});
        let saved = save_images(answer, folder.path());
        let note = saved["content"][2]["text"].as_str().unwrap_or_default().to_owned();
        let path = note.trim_start_matches("Saved to: ");
        assert_eq!(std::fs::read(path).unwrap_or_default(), b"jpeg");
    }
}
