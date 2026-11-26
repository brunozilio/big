use domain::lsp::ServerSpec;

const FOLDER: &str = "language-servers/";

pub fn shipped() -> Vec<ServerSpec> {
    let mut specs: Vec<ServerSpec> = Vec::new();
    for path in content::listed(FOLDER) {
        if !(path.ends_with(".ts") || path.ends_with(".js")) {
            continue;
        }
        let Some(declared) = content::value(&path, "spec") else {
            tracing::warn!(path, "a language server declares no spec");
            continue;
        };
        match serde_json::from_value::<ServerSpec>(declared) {
            Ok(spec) if !specs.iter().any(|known| known.name == spec.name) => specs.push(spec),
            Ok(spec) => tracing::warn!(path, name = spec.name, "a language server repeats another's name"),
            Err(error) => tracing::warn!(path, %error, "a language server spec does not load"),
        }
    }
    specs
}

#[cfg(test)]
mod tests {
    use domain::lsp::InstallSpec;

    use super::*;

    #[test]
    fn the_shipped_servers_come_from_content_with_their_recipes() {
        let specs = shipped();
        let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
        assert_eq!(names, ["clangd", "gopls", "pyright", "rust-analyzer", "typescript"]);
        let rust = specs.iter().find(|spec| spec.name == "rust-analyzer").expect("rust-analyzer");
        assert_eq!(rust.extension_to_language.get(".rs").map(String::as_str), Some("rust"));
        let Some(InstallSpec::Download(download)) = &rust.install else { panic!("a download: {:?}", rust.install) };
        let mac = download.platforms.get("macos-aarch64").expect("mac");
        assert!(mac.url.ends_with("/rust-analyzer-aarch64-apple-darwin.gz"), "{}", mac.url);
        assert_eq!(mac.sha256.len(), 64);
        assert!(specs.iter().all(|spec| spec.install.is_some()), "every shipped server can be installed");
    }
}
