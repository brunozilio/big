use std::path::{Path, PathBuf};

const FILES: [(&str, &str); 22] = [
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
    ("page/agent.js", include_str!("../extension/page/agent.js")),
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
}

pub fn install(folder: &Path, port: u16) -> Result<Installed, String> {
    let mut written = 0;
    let icon = icon();
    for (path, source) in FILES {
        let target = folder.join(path);
        let rendered = render(source, port);
        let rendered = if path == "manifest.json" && icon.is_none() { without_icons(&rendered) } else { rendered };
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
    Ok(Installed { folder: folder.to_path_buf(), written })
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
        assert_eq!(install(folder.path(), 4321).map(|installed| installed.written), Ok(0));
        assert_eq!(install(folder.path(), 9999).map(|installed| installed.written), Ok(1));
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
    fn without_a_png_the_manifest_keeps_no_icon() {
        let manifest = without_icons(&render(FILES[0].1, 4321));
        let parsed: serde_json::Value = serde_json::from_str(&manifest).unwrap_or_default();
        assert!(parsed.get("icons").is_none());
        assert!(parsed["action"].get("default_icon").is_none());
        assert_eq!(parsed["action"]["default_title"], brand::NAME);
    }
}
