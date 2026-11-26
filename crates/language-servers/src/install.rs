use std::path::PathBuf;

use domain::lsp::{ArchiveKind, InstallSpec, ServerSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archive {
    Gzip,
    Zip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipe {
    Npm { packages: Vec<String> },
    Go { module: String },
    Download { url: String, archive: Archive, sha256: String },
}

impl Recipe {
    pub const fn tool(&self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Npm { .. } => Some(("npm", "Node.js")),
            Self::Go { .. } => Some(("go", "Go")),
            Self::Download { .. } => None,
        }
    }

    pub fn described(&self) -> String {
        match self {
            Self::Npm { packages } => {
                format!("npm install --ignore-scripts {} (package scripts are not run)", packages.join(" "))
            }
            Self::Go { module } => format!("go install {module}"),
            Self::Download { url, sha256, .. } => {
                let start: String = sha256.chars().take(16).collect();
                format!("download {url}, checked against its pinned SHA-256 {start}…")
            }
        }
    }
}

pub const NPM_FLAGS: [&str; 4] = ["--ignore-scripts", "--no-audit", "--no-fund", "--loglevel=error"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    pub recipe: Recipe,
    pub program: PathBuf,
}

pub fn install_for(spec: &ServerSpec, os: &str, arch: &str) -> Option<Install> {
    let exe = if os == "windows" { ".exe" } else { "" };
    match spec.install.as_ref()? {
        InstallSpec::Npm(packages) => {
            let script = if os == "windows" { ".cmd" } else { "" };
            Some(Install {
                recipe: Recipe::Npm { packages: packages.clone() },
                program: PathBuf::from("node_modules").join(".bin").join(bare(&spec.command, script)?),
            })
        }
        InstallSpec::Go(module) => Some(Install {
            recipe: Recipe::Go { module: module.clone() },
            program: PathBuf::from("bin").join(bare(&spec.command, exe)?),
        }),
        InstallSpec::Download(download) => {
            let asset = download.platforms.get(&format!("{os}-{arch}"))?;
            let archive = match asset.archive {
                ArchiveKind::Gzip => Archive::Gzip,
                ArchiveKind::Zip => Archive::Zip,
            };
            let recipe = Recipe::Download { url: asset.url.clone(), archive, sha256: asset.sha256.clone() };
            Some(Install { recipe, program: inside(&download.program, exe)? })
        }
    }
}

fn bare(command: &str, suffix: &str) -> Option<PathBuf> {
    inside(command, suffix).filter(|program| program.components().count() == 1)
}

fn inside(program: &str, exe: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = program.split('/').collect();
    let plain = |part: &&str| !part.is_empty() && *part != "." && *part != ".." && !part.contains(['\\', ':']);
    if !parts.iter().all(plain) {
        return None;
    }
    let (name, folders) = parts.split_last()?;
    Some(folders.iter().collect::<PathBuf>().join(format!("{name}{exe}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, command: &str, install: serde_json::Value) -> ServerSpec {
        serde_json::from_value(serde_json::json!({
            "name": name, "command": command, "extensionToLanguage": {".x": "x"}, "install": install
        }))
        .expect("spec")
    }

    fn downloaded() -> ServerSpec {
        spec(
            "tool",
            "tool",
            serde_json::json!({"download": {"program": "tool_1/bin/tool", "platforms": {
                "macos-aarch64": {"url": "https://example.com/tool-mac.gz", "archive": "gzip", "sha256": "aa11"},
                "windows-x86_64": {"url": "https://example.com/tool-win.zip", "archive": "zip", "sha256": "bb22"}
            }}}),
        )
    }

    #[test]
    fn a_download_takes_the_asset_of_this_platform_with_its_checksum() {
        let mac = install_for(&downloaded(), "macos", "aarch64").expect("mac");
        assert_eq!(
            mac,
            Install {
                recipe: Recipe::Download {
                    url: "https://example.com/tool-mac.gz".into(),
                    archive: Archive::Gzip,
                    sha256: "aa11".into()
                },
                program: PathBuf::from("tool_1").join("bin").join("tool"),
            }
        );
        let windows = install_for(&downloaded(), "windows", "x86_64").expect("windows");
        assert_eq!(windows.program, PathBuf::from("tool_1").join("bin").join("tool.exe"));
        assert!(matches!(windows.recipe, Recipe::Download { archive: Archive::Zip, .. }));
        assert_eq!(install_for(&downloaded(), "linux", "x86_64"), None, "no asset for this platform");
    }

    #[test]
    fn npm_and_go_recipes_put_the_program_where_their_tool_installs_it() {
        let node = spec("ts", "ts-server", serde_json::json!({"npm": ["ts-server@1.0.0", "ts@1.0.0"]}));
        assert_eq!(
            install_for(&node, "windows", "x86_64"),
            Some(Install {
                recipe: Recipe::Npm { packages: vec!["ts-server@1.0.0".into(), "ts@1.0.0".into()] },
                program: PathBuf::from("node_modules").join(".bin").join("ts-server.cmd"),
            })
        );
        let go = spec("gopls", "gopls", serde_json::json!({"go": "example.com/gopls@v1"}));
        assert_eq!(
            install_for(&go, "linux", "aarch64"),
            Some(Install {
                recipe: Recipe::Go { module: "example.com/gopls@v1".into() },
                program: PathBuf::from("bin").join("gopls")
            })
        );
    }

    #[test]
    fn a_recipe_names_the_tool_it_runs_and_says_what_it_will_do() {
        let node = install_for(&spec("p", "p", serde_json::json!({"npm": ["p@1"]})), "linux", "x86_64").expect("npm");
        assert_eq!(node.recipe.tool(), Some(("npm", "Node.js")));
        assert_eq!(node.recipe.described(), "npm install --ignore-scripts p@1 (package scripts are not run)");
        let go = install_for(&spec("g", "g", serde_json::json!({"go": "example.com/g@v1"})), "linux", "x86_64");
        assert_eq!(go.expect("go").recipe.described(), "go install example.com/g@v1");
        let download = install_for(&downloaded(), "macos", "aarch64").expect("download").recipe;
        assert_eq!(download.tool(), None);
        assert!(download.described().starts_with("download https://") && download.described().contains("SHA-256 aa11"));
        assert!(NPM_FLAGS.contains(&"--ignore-scripts"));
    }

    #[test]
    fn a_server_without_a_recipe_is_never_installed() {
        let mut plain = downloaded();
        plain.install = None;
        assert_eq!(install_for(&plain, "macos", "aarch64"), None);
        let escaping = spec("e", "e", serde_json::json!({"download": {"program": "../../bin/e", "platforms": {
            "macos-aarch64": {"url": "https://example.com/e.gz", "archive": "gzip", "sha256": "cc"}}}}));
        assert_eq!(install_for(&escaping, "macos", "aarch64"), None);
        let pathed = spec("n", "/opt/n", serde_json::json!({"npm": ["n@1"]}));
        assert_eq!(install_for(&pathed, "macos", "aarch64"), None, "a command with a path is not what npm installs");
    }
}
