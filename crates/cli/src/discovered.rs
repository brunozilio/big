use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Arg, Command, value_parser};
use serde::Deserialize;

pub const MANIFEST: &str = "commands.json";
pub const ARGS: &str = "args";

#[derive(Debug, Deserialize)]
struct Declared {
    #[serde(default)]
    about: String,
    run: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovered {
    pub name: String,
    pub about: String,
    pub run: Vec<String>,
    pub folder: PathBuf,
}

pub fn roots(listed: &str, base: &Path) -> Vec<PathBuf> {
    listed.split(',').map(str::trim).filter(|root| !root.is_empty()).map(|root| base.join(root)).collect()
}

pub fn discover(roots: &[PathBuf]) -> Vec<Discovered> {
    let mut found: BTreeMap<String, Discovered> = BTreeMap::new();
    for folder in roots.iter().flat_map(|root| folders(root)) {
        for command in declared(&folder) {
            found.entry(command.name.clone()).or_insert(command);
        }
    }
    found.into_values().collect()
}

fn folders(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|folder| folder.join(MANIFEST).is_file())
        .collect();
    folders.sort();
    folders
}

fn named(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn declared(folder: &Path) -> Vec<Discovered> {
    let manifest = folder.join(MANIFEST);
    let Ok(text) = std::fs::read_to_string(&manifest) else { return Vec::new() };
    let parsed: BTreeMap<String, Declared> = match serde_json::from_str(&text) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{}: {error}", manifest.display());
            return Vec::new();
        }
    };
    parsed
        .into_iter()
        .filter(|(name, declared)| named(name) && !declared.run.is_empty())
        .map(|(name, declared)| Discovered {
            name,
            about: declared.about,
            run: declared.run,
            folder: folder.to_path_buf(),
        })
        .collect()
}

pub fn injected(mut cli: Command, found: &[Discovered]) -> Command {
    for command in found {
        if cli.find_subcommand(&command.name).is_some() {
            continue;
        }
        let args = Arg::new(ARGS)
            .num_args(0..)
            .trailing_var_arg(true)
            .allow_hyphen_values(true)
            .value_parser(value_parser!(OsString));
        cli = cli.subcommand(
            Command::new(command.name.clone()).about(command.about.clone()).disable_help_flag(true).arg(args),
        );
    }
    cli
}

fn program(command: &Discovered) -> Result<(PathBuf, &[String]), String> {
    let (first, rest) = command.run.split_first().ok_or_else(|| format!("{}: nothing to run", command.name))?;
    let given = PathBuf::from(first);
    let program =
        if given.is_relative() && given.components().count() > 1 { command.folder.join(given) } else { given };
    Ok((program, rest))
}

pub fn run(command: &Discovered, args: Vec<OsString>, root: &Path) -> Result<ExitCode, String> {
    let (program, fixed) = program(command)?;
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let status = std::process::Command::new(&program)
        .args(fixed)
        .args(args)
        .current_dir(&command.folder)
        .env("COMMAND_EXE", exe)
        .env("COMMAND_NAME", brand::COMMAND)
        .env("COMMAND_ROOT", root)
        .status()
        .map_err(|error| format!("{}: {error}", program.display()))?;
    Ok(status.code().and_then(|code| u8::try_from(code).ok()).map_or(ExitCode::FAILURE, ExitCode::from))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "a failed setup step stops the test")]

    use super::*;

    fn folder_with(root: &Path, name: &str, manifest: &str) {
        let folder = root.join(name);
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(folder.join(MANIFEST), manifest).expect("manifest");
    }

    #[test]
    fn a_folder_with_a_manifest_brings_its_commands() {
        let root = tempfile::tempdir().expect("root");
        folder_with(root.path(), "b", r#"{"deploy-x": {"about": "B", "run": ["sh", "b.sh"]}}"#);
        folder_with(
            root.path(),
            "a",
            r#"{"deploy-x": {"about": "A", "run": ["sh"]}, "Bad": {"run": ["x"]}, "empty": {"run": []}}"#,
        );
        folder_with(root.path(), "broken", "{");
        std::fs::create_dir_all(root.path().join("plain")).expect("plain");
        let found = discover(&roots(" missing , . ", root.path()));
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].about.as_str(), found[0].folder.ends_with("a")), ("A", true));
    }

    #[test]
    fn a_relative_program_runs_from_its_folder() {
        let command = Discovered {
            name: "x".into(),
            about: String::new(),
            run: vec!["./run.sh".into(), "go".into()],
            folder: PathBuf::from("/opt/x"),
        };
        let (program, rest) = program(&command).expect("program");
        assert_eq!((program, rest), (PathBuf::from("/opt/x/./run.sh"), &["go".to_owned()][..]));
        let plain = Discovered { run: vec!["bun".into()], ..command };
        assert_eq!(super::program(&plain).expect("program").0, PathBuf::from("bun"));
    }
}
