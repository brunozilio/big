use std::path::PathBuf;

fn synced(manifest: &str, command: &str) -> Option<String> {
    let (head, bin) = manifest.split_once("[[bin]]")?;
    let line = bin.lines().find(|line| line.trim_start().starts_with("name"))?;
    let wanted = format!("name = {command:?}");
    (line.trim() != wanted).then(|| format!("{head}[[bin]]{}", bin.replacen(line, &wanted, 1)))
}

fn main() {
    let path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", path.display());
    let manifest = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let Some(updated) = synced(&manifest, brand::COMMAND) else { return };
    std::fs::write(&path, updated).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    panic!("{} now names the binary `{}`, the `command` in config.toml; build again", path.display(), brand::COMMAND);
}
