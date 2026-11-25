use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use domain::PermissionMode;
use domain::team::{LEAD, file_name};

use super::Team;

pub const SUBCOMMAND: &str = "teammate";

#[derive(Debug, Clone)]
pub struct Launching<'a> {
    pub team: &'a Team,
    pub name: &'a str,
    pub cwd: &'a Path,
    pub prompt: &'a str,
    pub instructions: &'a str,
    pub model: &'a str,
    pub mode: PermissionMode,
    pub lead: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub log: PathBuf,
    pub spec: (PathBuf, domain::team::TeammateSpec),
}

fn spec(launching: &Launching<'_>) -> (PathBuf, domain::team::TeammateSpec) {
    let path = launching.team.dir.join("spawn").join(format!("{}.json", file_name(launching.name)));
    let details = domain::team::TeammateSpec {
        team: launching.team.name.clone(),
        name: launching.name.to_owned(),
        cwd: launching.cwd.display().to_string(),
        model: launching.model.to_owned(),
        mode: domain::command::mode_label(launching.mode).to_owned(),
        instructions: launching.instructions.to_owned(),
        lead_pid: Some(std::process::id()),
        lead: launching.lead.map(str::to_owned),
    };
    (path, details)
}

fn teammate_args(spec: &Path) -> Vec<OsString> {
    vec![SUBCOMMAND.into(), "--spec".into(), spec.as_os_str().to_owned()]
}

pub fn planned(launching: &Launching<'_>, program: PathBuf) -> Planned {
    let spec = spec(launching);
    let log = launching.team.dir.join("logs").join(format!("{}.log", file_name(launching.name)));
    Planned { program, args: teammate_args(&spec.0), log, spec }
}

fn program() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|error| format!("the app's own program cannot be found: {error}"))
}

pub fn launch(launching: &Launching<'_>) -> Result<(), String> {
    launching.team.send(launching.name, LEAD, launching.prompt, None)?;
    let planned = planned(launching, program()?);
    let (path, spec) = &planned.spec;
    platform::store::Document::at(path).write(spec).map_err(|error| error.to_string())?;
    let mut command = Command::new(&planned.program);
    command.args(&planned.args).current_dir(launching.cwd).stdin(Stdio::null());
    let log = &planned.log;
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    let out = std::fs::File::create(log).map_err(|error| error.to_string())?;
    let errors = out.try_clone().map_err(|error| error.to_string())?;
    let mut child =
        command.stdout(out).stderr(errors).spawn().map_err(|error| format!("the teammate could not start: {error}"))?;
    let team = launching.team.clone();
    let name = launching.name.to_owned();
    std::thread::Builder::new()
        .name(format!("teammate-{name}"))
        .spawn(move || {
            let _ = child.wait();
            team.departed(&name);
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launching(team: &Team) -> Launching<'_> {
        Launching {
            team,
            name: "tester",
            cwd: Path::new("/work"),
            prompt: "fix the tests",
            instructions: "# Team coordination",
            model: "claude-sonnet-5-5",
            mode: PermissionMode::Auto,
            lead: Some("planner"),
        }
    }

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn a_teammate_runs_the_app_s_own_program_in_its_team_with_its_name_mode_and_instructions() {
        let team = Team::named(Path::new("/home/me"), "team-1");

        let planned = planned(&launching(&team), PathBuf::from("/bin/app"));

        let args = strings(&planned.args);
        let spec_file = format!("/home/me/{}/teams/team-1/spawn/tester.json", brand::DIR);
        assert_eq!(args, ["teammate", "--spec", spec_file.as_str()]);
        let (path, spec) = &planned.spec;
        assert_eq!(path, &PathBuf::from(&spec_file));
        assert_eq!((spec.team.as_str(), spec.name.as_str(), spec.cwd.as_str()), ("team-1", "tester", "/work"));
        assert_eq!((spec.model.as_str(), spec.instructions.as_str()), ("claude-sonnet-5-5", "# Team coordination"));
        assert_eq!(spec.mode, domain::command::mode_label(PermissionMode::Auto));
        assert_eq!(spec.lead.as_deref(), Some("planner"));
        assert!(planned.log.ends_with("teams/team-1/logs/tester.log"));
        assert!(!args.iter().any(|arg| arg.contains("fix the tests")), "the task goes through the mailbox");
    }
}
