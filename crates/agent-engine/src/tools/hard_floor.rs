use std::path::{Path, PathBuf};

use domain::permission::hard::{Category, Declared, Floor, Patterns};
use serde_json::Value;

use super::outside::settled;

const FILE: &str = "safety/hard.ts";
const TARGETS: &str = "safety/targets/";
const EXPORT: &str = "hard";

pub fn floor(home: Option<&Path>) -> Floor {
    let home = home.map(Path::to_path_buf).or_else(std::env::home_dir);
    let mut floor = Floor::at_home(home.clone());
    floor.govern(governed(home.as_deref()), [brand::DIR.to_owned()]);
    for (name, source) in sources() {
        let homeless = floor.add(&declared(&name, &source), &|path| settled(path));
        if !homeless.is_empty() {
            tracing::warn!(
                file = name,
                ?homeless,
                "no home folder is known, so these safety floor paths match wherever they appear"
            );
        }
    }
    floor
}

fn governed(home: Option<&Path>) -> Vec<PathBuf> {
    let mut places: Vec<PathBuf> = home.map(|home| home.join(brand::DIR)).into_iter().collect();
    places.extend(content::home());
    places.extend(content::folder());
    if content::mode() == content::Mode::Live {
        let live = content::live_root();
        places.push(live.to_path_buf());
        places.extend(live.parent().map(|repo| repo.join("config.toml")));
    }
    places.iter().map(|place| settled(place)).collect()
}

fn shipped_names() -> Vec<String> {
    let mut names: Vec<String> = content::paths().into_iter().map(str::to_owned).collect();
    names.retain(|name| name == FILE || (name.starts_with(TARGETS) && name.ends_with(".ts")));
    names.sort();
    names.dedup();
    names
}

fn user_names(folder: &Path) -> Vec<String> {
    let listed = std::fs::read_dir(folder.join(TARGETS)).into_iter().flatten().flatten();
    let mut names: Vec<String> = listed
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter(|name| name.ends_with(".ts"))
        .map(|name| format!("{TARGETS}{name}"))
        .collect();
    names.push(FILE.to_owned());
    names
}

fn sources() -> Vec<(String, String)> {
    let text = |bytes: &[u8]| brand::text(&String::from_utf8_lossy(bytes));
    let mut found = Vec::new();
    for name in shipped_names() {
        found.extend(content::builtin(&name).map(|bytes| (name.clone(), text(bytes))));
        found.extend(content::shipped(&name).map(|bytes| (name.clone(), text(&bytes))));
    }
    if let Some(folder) = content::folder() {
        for name in user_names(&folder) {
            found.extend(content::user_text(Some(&folder), &name).map(|source| (name.clone(), source)));
        }
    }
    found
}

fn declared(name: &str, source: &str) -> Declared {
    let value = script::exported(name, source, EXPORT)
        .inspect_err(|error| tracing::warn!(file = name, %error, "a safety floor file does not load"))
        .unwrap_or(Value::Null);
    let Value::Object(categories) = value else { return Declared::new() };
    categories
        .into_iter()
        .filter_map(|(key, patterns)| {
            let category = serde_json::from_value::<Category>(Value::String(key.clone()))
                .inspect_err(|_| tracing::warn!(file = name, category = key, "an unknown safety floor category"))
                .ok()?;
            let patterns = serde_json::from_value::<Patterns>(patterns)
                .inspect_err(|error| tracing::warn!(file = name, %error, "safety floor patterns do not read"))
                .ok()?;
            Some((category, patterns))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use domain::permission::{Call, Effect, Place, Reason, Verdict};

    use super::*;

    #[test]
    fn the_shipped_floor_reads_every_file_and_a_copy_without_a_pattern_frees_nothing() {
        let home = tempfile::tempdir().expect("home");
        let floor = floor(Some(home.path()));
        let read = |floor: &Floor, key: &str| {
            let effects = [Effect::ReadPath(Place::at(settled(&home.path().join(key))))];
            floor.verdict(&Call { tool: "Read", subject: "", effects: &effects })
        };
        for key in [".ssh/id_rsa", ".aws/credentials", ".npmrc"] {
            assert!(read(&floor, key).is_some_and(|verdict| verdict.denies()), "{key}");
        }
        assert_eq!(read(&floor, "notes.md"), None);
        let effects = [Effect::exec("NODE_OPTIONS=--require x node a.js")];
        let asked = floor.verdict(&Call { tool: "Bash", subject: "", effects: &effects });
        assert!(matches!(asked, Some(Verdict::Ask { reason: Reason::Hard(_), rememberable: false })), "{asked:?}");
        let emptied = declared(FILE, "export const hard = { credentials: { paths: [] } };");
        let mut lowered = floor.clone();
        lowered.add(&emptied, &|path| settled(path));
        assert!(read(&lowered, ".ssh/id_rsa").is_some_and(|verdict| verdict.denies()));
        let found = sources();
        assert!(found.iter().any(|(name, _)| name == FILE), "the built-in copy is always read");
        assert!(found.iter().any(|(name, _)| name.starts_with(TARGETS)));
    }
}
