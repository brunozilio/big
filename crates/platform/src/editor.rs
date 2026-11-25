use std::path::{Path, PathBuf};

use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub name: String,
    pub path: PathBuf,
}

pub trait Editor: Send + Sync {
    fn name(&self) -> &str;

    fn file(&self, file: &Path, line: u64, column: u64) -> String;

    fn folders(&self, folders: &[Folder], workspace: &Path) -> std::io::Result<String>;
}

pub fn chosen(kind: &str, name: &str, scheme: &str) -> Option<Box<dyn Editor>> {
    match kind {
        "vscode" => Some(Box::new(VsCode { name: name.to_owned(), scheme: scheme.to_owned() })),
        _ => None,
    }
}

pub fn escaped(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let rooted = if text.starts_with('/') { text } else { format!("/{text}") };
    let keep = |byte: u8| byte.is_ascii_alphanumeric() || b"/-_.~:".contains(&byte);
    rooted.bytes().map(|byte| if keep(byte) { (byte as char).to_string() } else { format!("%{byte:02X}") }).collect()
}

struct VsCode {
    name: String,
    scheme: String,
}

impl VsCode {
    fn url(&self, path: &Path) -> String {
        format!("{}://file{}", self.scheme, escaped(path))
    }
}

impl Editor for VsCode {
    fn name(&self) -> &str {
        &self.name
    }

    fn file(&self, file: &Path, line: u64, column: u64) -> String {
        format!("{}:{}:{}", self.url(file), line.max(1), column.max(1))
    }

    fn folders(&self, folders: &[Folder], workspace: &Path) -> std::io::Result<String> {
        if let [only] = folders {
            return Ok(self.url(&only.path));
        }
        let listed: Vec<_> = folders
            .iter()
            .map(|folder| json!({ "name": folder.name, "path": folder.path.to_string_lossy() }))
            .collect();
        let file = workspace.with_extension("code-workspace");
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&json!({ "folders": listed, "settings": {} }))?;
        crate::atomic::write(&file, text.as_bytes())?;
        Ok(self.url(&file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vscode() -> Box<dyn Editor> {
        chosen("vscode", "VS Code", "vscode").expect("editor")
    }

    #[test]
    fn a_file_url_carries_the_escaped_path_and_the_position() {
        let url = vscode().file(Path::new("/Users/me/Application Support/a b.rs"), 91, 0);
        assert_eq!(url, "vscode://file/Users/me/Application%20Support/a%20b.rs:91:1");
    }

    #[test]
    fn one_folder_opens_by_itself() {
        let dir = tempfile::tempdir().expect("dir");
        let only = [Folder { name: "api".into(), path: PathBuf::from("/w/api") }];
        let url = vscode().folders(&only, &dir.path().join("item")).expect("url");
        assert_eq!(url, "vscode://file/w/api");
        assert!(!dir.path().join("item.code-workspace").exists());
    }

    #[test]
    fn several_folders_open_as_a_named_workspace() {
        let dir = tempfile::tempdir().expect("dir");
        let both = [
            Folder { name: "api".into(), path: PathBuf::from("/w/api") },
            Folder { name: "web".into(), path: PathBuf::from("/w/web") },
        ];
        let url = vscode().folders(&both, &dir.path().join("spaces/item")).expect("url");
        let file = dir.path().join("spaces/item.code-workspace");
        assert_eq!(url, format!("vscode://file{}", escaped(&file)));
        let kept: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).expect("read")).expect("json");
        assert_eq!(kept["folders"][1], json!({ "name": "web", "path": "/w/web" }));
    }

    #[test]
    fn an_unknown_kind_has_no_editor() {
        assert!(chosen("notepad", "x", "x").is_none());
    }
}
