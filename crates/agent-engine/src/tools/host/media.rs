use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use platform::bounded::{Bounds, Ended};
use serde_json::{Value, json};

use super::{Reach, text};

pub const SPACE: &str = "media";

const MOST: u64 = 64 * 1024 * 1024;

pub fn call(reach: &Reach<'_>, name: &str, arguments: &Value) -> Result<Value, String> {
    match name {
        "load" => load(reach, arguments),
        "pdf" => pdf(reach, arguments),
        other => Err(format!("ctx.{SPACE}.{other} is not a function")),
    }
}

fn load(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let place = reach.workspace.place(text(arguments, "path")?);
    reach.fence.reads(&place.real)?;
    let bytes = read_bytes(&place.real, &reach.workspace.stop).map_err(|error| error.to_string())?;
    if arguments.get("plain").and_then(Value::as_bool) != Some(true) {
        match domain::Image::from_bytes(&bytes) {
            Ok(Some(image)) => return Ok(json!({ "image": image })),
            Err(error) => return Ok(json!({ "imageError": error })),
            Ok(None) => {}
        }
        if bytes.starts_with(b"%PDF") {
            return Ok(json!({ "pdf": true }));
        }
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok(json!({ "text": text })),
        Err(_) => Ok(json!({ "binary": true })),
    }
}

pub(super) fn read_bytes(path: &Path, stop: &AtomicBool) -> std::io::Result<Vec<u8>> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    platform::nonblocking(&mut options);
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::other("Read requires a regular file, not a pipe or device"));
    }
    if metadata.len() > MOST {
        return Err(std::io::Error::other("file exceeds the 64 MiB read limit"));
    }
    let mut result = Vec::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("file read cancelled"));
        }
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        result.extend_from_slice(&buffer[..count]);
        if result.len() as u64 > MOST {
            return Err(std::io::Error::other("file exceeds the 64 MiB read limit"));
        }
    }
    Ok(result)
}

fn pdf(reach: &Reach<'_>, arguments: &Value) -> Result<Value, String> {
    let place = reach.workspace.place(text(arguments, "path")?);
    reach.fence.reads(&place.real)?;
    let mut command = platform::command("pdftotext");
    command.arg("-layout");
    let page = |key: &str| arguments.get(key).and_then(Value::as_u64);
    if let (Some(first), Some(last)) = (page("first"), page("last")) {
        command.args(["-f", &first.to_string(), "-l", &last.to_string()]);
    }
    command.arg(&place.real).arg("-");
    let bounds =
        Bounds { timeout: Duration::from_secs(60), max_output: 8 << 20, stop: &reach.workspace.stop, input: None };
    let output = platform::bounded::run(&mut command, bounds).map_err(|_| {
        "Reading a PDF needs `pdftotext` (poppler: `brew install poppler`, `apt install poppler-utils`).".to_owned()
    })?;
    match output.ended {
        Ended::Exited(status) if status.success() => Ok(Value::String(output.stdout)),
        Ended::Exited(_) => Err(format!("pdftotext could not read it: {}", output.stderr.trim())),
        Ended::Stopped => Err("Stopped reading the PDF.".into()),
        Ended::TimedOut => Err("pdftotext took over a minute; try fewer pages.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::Fence;
    use super::*;
    use crate::tools::{Effect, Workspace};

    #[test]
    fn a_file_loads_as_an_image_a_pdf_text_or_bytes() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path();
        std::fs::write(root.join("a.png"), b"\x89PNG\r\n\x1a\n rest").expect("png");
        std::fs::write(root.join("b.pdf"), b"%PDF-1.4").expect("pdf");
        std::fs::write(root.join("c.bin"), [0xff, 0xfe]).expect("bin");
        let workspace = Workspace::new(root);
        let effects = [Effect::ReadTree(workspace.place("."))];
        let reach = Reach { workspace: &workspace, fence: Fence::of(&effects), streams: None };
        let loaded = |path: &str| call(&reach, "load", &json!({ "path": path })).expect("load");
        assert_eq!(loaded("a.png")["image"]["media_type"], "image/png");
        assert_eq!(loaded("b.pdf"), json!({ "pdf": true }));
        assert_eq!(loaded("c.bin"), json!({ "binary": true }));
        assert_eq!(call(&reach, "load", &json!({ "path": "b.pdf", "plain": true })), Ok(json!({ "text": "%PDF-1.4" })));
        assert!(call(&reach, "load", &json!({ "path": "missing" })).is_err());
        let fenced = Reach { workspace: &workspace, fence: Fence::of(&[]), streams: None };
        assert!(call(&fenced, "load", &json!({ "path": "a.png" })).is_err());
    }
}
