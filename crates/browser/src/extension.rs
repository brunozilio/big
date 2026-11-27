use std::path::{Path, PathBuf};

const FILES: [(&str, &str); 25] = [
    ("manifest.json", include_str!("../extension/manifest.json")),
    ("config.js", include_str!("../extension/config.js")),
    ("bg/main.js", include_str!("../extension/bg/main.js")),
    ("bg/link.js", include_str!("../extension/bg/link.js")),
    ("bg/tabs.js", include_str!("../extension/bg/tabs.js")),
    ("bg/cdp.js", include_str!("../extension/bg/cdp.js")),
    ("bg/computer.js", include_str!("../extension/bg/computer.js")),
    ("bg/keys.js", include_str!("../extension/bg/keys.js")),
    ("bg/page.js", include_str!("../extension/bg/page.js")),
    ("bg/results.js", include_str!("../extension/bg/results.js")),
    ("bg/logs.js", include_str!("../extension/bg/logs.js")),
    ("bg/capture.js", include_str!("../extension/bg/capture.js")),
    ("bg/intercept.js", include_str!("../extension/bg/intercept.js")),
    ("bg/frame.js", include_str!("../extension/bg/frame.js")),
    ("bg/frames.js", include_str!("../extension/bg/frames.js")),
    ("bg/sites.js", include_str!("../extension/bg/sites.js")),
    ("bg/ask.js", include_str!("../extension/bg/ask.js")),
    ("bg/shortcuts.js", include_str!("../extension/bg/shortcuts.js")),
    ("bg/relay.js", include_str!("../extension/bg/relay.js")),
    ("bg/servers.js", include_str!("../extension/bg/servers.js")),
    ("page/agent.js", include_str!("../extension/page/agent.js")),
    ("page/relay.js", include_str!("../extension/page/relay.js")),
    ("ui/ask.html", include_str!("../extension/ui/ask.html")),
    ("ui/ask.js", include_str!("../extension/ui/ask.js")),
    ("ui/style.css", include_str!("../extension/ui/style.css")),
];

const ICON: &str = "icon.png";
const GONE: [&str; 2] = ["ui/popup.html", "ui/popup.js"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub folder: PathBuf,
    pub written: usize,
    pub build: String,
}

pub fn install(folder: &Path, port: u16) -> Result<Installed, String> {
    let mut written = 0;
    let icon = icon();
    let rendered = rendered(port, icon.is_none());
    let build = fingerprint(&rendered, icon);
    for (path, rendered) in rendered {
        let target = folder.join(path);
        let rendered = rendered.replace("{{BUILD}}", &build);
        if std::fs::read_to_string(&target).is_ok_and(|current| current == rendered) {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        std::fs::write(&target, rendered).map_err(|error| format!("{}: {error}", target.display()))?;
        written += 1;
    }
    match icon {
        Some(bytes) => written += usize::from(write_if_changed(&folder.join(ICON), bytes)?),
        None => {
            let _ = std::fs::remove_file(folder.join(ICON));
        }
    }
    for gone in GONE {
        let _ = std::fs::remove_file(folder.join(gone));
    }
    Ok(Installed { folder: folder.to_path_buf(), written, build })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packaged {
    pub name: String,
    pub bytes: Vec<u8>,
    pub build: String,
}

pub fn packaged() -> Result<Packaged, String> {
    let icon = icon();
    let rendered = rendered(0, icon.is_none());
    let build = fingerprint(&rendered, icon);
    let folder = format!("{}-{}", brand::COMMAND, brand::browser::EXTENSION_FOLDER);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let failed = |error: zip::result::ZipError| error.to_string();
    let mut entries: Vec<(String, Vec<u8>)> = rendered
        .into_iter()
        .map(|(path, text)| (path.to_owned(), text.replace("{{BUILD}}", &build).into_bytes()))
        .collect();
    if let Some(bytes) = icon {
        entries.push((ICON.to_owned(), bytes.to_vec()));
    }
    for (path, bytes) in entries {
        zip.start_file(format!("{folder}/{path}"), options).map_err(failed)?;
        std::io::Write::write_all(&mut zip, &bytes).map_err(|error| error.to_string())?;
    }
    let bytes = zip.finish().map_err(failed)?.into_inner();
    Ok(Packaged { name: format!("{folder}.zip"), bytes, build })
}

fn rendered(port: u16, iconless: bool) -> Vec<(&'static str, String)> {
    FILES
        .iter()
        .map(|(path, source)| {
            let text = render(source, port);
            (*path, if *path == "manifest.json" && iconless { without_icons(&text) } else { text })
        })
        .collect()
}

fn fingerprint(files: &[(&str, String)], icon: Option<&[u8]>) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    for (path, text) in files {
        feed(path.as_bytes());
        feed(&[0]);
        feed(text.as_bytes());
        feed(&[0]);
    }
    feed(icon.unwrap_or_default());
    format!("{hash:016x}")
}

fn icon() -> Option<&'static [u8]> {
    [brand::LOGO, brand::FAVICON].into_iter().find(brand::Image::is_png).map(|image| image.bytes)
}

fn without_icons(manifest: &str) -> String {
    let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(manifest) else { return manifest.to_owned() };
    if let Some(object) = parsed.as_object_mut() {
        object.remove("icons");
    }
    if let Some(action) = parsed.get_mut("action").and_then(serde_json::Value::as_object_mut) {
        action.remove("default_icon");
    }
    serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| manifest.to_owned())
}

fn write_if_changed(target: &Path, bytes: &[u8]) -> Result<bool, String> {
    if std::fs::read(target).is_ok_and(|current| current == bytes) {
        return Ok(false);
    }
    std::fs::write(target, bytes).map_err(|error| format!("{}: {error}", target.display()))?;
    Ok(true)
}

fn render(source: &str, port: u16) -> String {
    brand::text(source).replace("{{PORT}}", &port.to_string()).replace("{{VERSION}}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_is_written_with_the_brand_and_port_and_rewritten_only_when_it_changes() {
        let folder = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let first = install(folder.path(), 4321);
        assert_eq!(first.map(|installed| installed.written), Ok(FILES.len() + usize::from(icon().is_some())));
        assert_eq!(std::fs::read(folder.path().join(ICON)).ok().as_deref(), icon());
        let manifest = std::fs::read_to_string(folder.path().join("manifest.json")).unwrap_or_default();
        let parsed: serde_json::Value = serde_json::from_str(&manifest).unwrap_or_default();
        assert_eq!(parsed["manifest_version"], 3);
        assert_eq!(parsed["name"], brand::NAME);
        if icon().is_some() {
            assert_eq!(parsed["icons"]["128"], ICON);
        }
        assert!(parsed["action"].get("default_popup").is_none());
        let config = std::fs::read_to_string(folder.path().join("config.js")).unwrap_or_default();
        assert!(config.contains("4321"));
        for (path, _) in FILES {
            let text = std::fs::read_to_string(folder.path().join(path)).unwrap_or_default();
            assert!(!text.contains("{{"), "{path} keeps a placeholder");
        }
        let again = install(folder.path(), 4321);
        assert_eq!(again.as_ref().map(|installed| installed.written), Ok(0));
        let build = again.map(|installed| installed.build).unwrap_or_default();
        assert_eq!(build.len(), 16);
        assert!(config.contains(&build));
        let moved = install(folder.path(), 9999);
        assert_eq!(moved.as_ref().map(|installed| installed.written), Ok(2));
        assert_ne!(moved.map(|installed| installed.build).unwrap_or_default(), build);
    }

    #[test]
    fn the_package_holds_every_file_in_one_folder_without_a_local_port() {
        let made = packaged().unwrap_or_else(|_| unreachable!());
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(made.bytes.clone())).unwrap_or_else(|_| unreachable!());
        assert_eq!(archive.len(), FILES.len() + usize::from(icon().is_some()));
        let folder = format!("{}-{}", brand::COMMAND, brand::browser::EXTENSION_FOLDER);
        assert_eq!(made.name, format!("{folder}.zip"));
        let mut config = String::new();
        let entry = archive.by_name(&format!("{folder}/config.js"));
        std::io::Read::read_to_string(&mut entry.unwrap_or_else(|_| unreachable!()), &mut config).unwrap_or_default();
        assert!(config.contains("PORT = 0;"));
        assert!(config.contains(&made.build));
        assert_eq!(packaged().map(|again| again.build), Ok(made.build));
    }

    #[test]
    fn every_file_of_the_extension_folder_is_installed() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("extension");
        let mut pending = vec![root.clone()];
        let mut found = Vec::new();
        while let Some(folder) = pending.pop() {
            for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                } else if let Ok(relative) = path.strip_prefix(&root) {
                    found.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let missing: Vec<&String> = found.iter().filter(|path| !FILES.iter().any(|(known, _)| known == path)).collect();
        assert!(missing.is_empty(), "not installed: {missing:?}");
    }

    #[test]
    fn the_sign_in_relay_is_packaged_and_declared() {
        let packaged: Vec<&str> = FILES.iter().map(|(path, _)| *path).collect();
        assert!(packaged.contains(&"page/relay.js"));
        assert!(packaged.contains(&"bg/relay.js"));
        let manifest: serde_json::Value = serde_json::from_str(&render(FILES[0].1, 4321)).unwrap_or_default();
        let permissions = manifest["permissions"].as_array().cloned().unwrap_or_default();
        assert!(permissions.iter().any(|permission| permission == "declarativeNetRequest"));
        let scripts = manifest["content_scripts"].as_array().cloned().unwrap_or_default();
        let relay = scripts
            .iter()
            .find(|script| script["js"].as_array().is_some_and(|js| js.iter().any(|file| file == "page/relay.js")));
        let relay = relay.cloned().unwrap_or_default();
        assert_eq!(relay["matches"][0], "<all_urls>");
        assert_eq!(relay["run_at"], "document_start");
        assert_eq!(relay["all_frames"], false);
        let marker = format!("{}:relay", brand::NAME);
        let script = FILES
            .iter()
            .find(|(path, _)| *path == "page/relay.js")
            .map(|(_, source)| render(source, 4321))
            .unwrap_or_default();
        assert!(script.contains(&format!("\"{marker}\"")));
        let main = FILES.iter().find(|(path, _)| *path == "bg/main.js").map(|(_, source)| *source).unwrap_or_default();
        assert!(main.contains("from \"./relay.js\""));
    }

    #[test]
    fn without_a_png_the_manifest_keeps_no_icon() {
        let manifest = without_icons(&render(FILES[0].1, 4321));
        let parsed: serde_json::Value = serde_json::from_str(&manifest).unwrap_or_default();
        assert!(parsed.get("icons").is_none());
        assert!(parsed["action"].get("default_icon").is_none());
        assert_eq!(parsed["action"]["default_title"], brand::NAME);
    }
}
