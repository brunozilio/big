use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use domain::PermissionMode;
use domain::team::{LEAD, TeammateMode, file_name};

use super::Team;

pub const SUBCOMMAND: &str = "teammate";
const TMUX: &str = "tmux";

#[derive(Debug, Clone)]
pub struct Launching<'a> {
    pub team: &'a Team,
    pub name: &'a str,
    pub cwd: &'a Path,
    pub prompt: &'a str,
    pub instructions: &'a str,
    pub model: &'a str,
    pub mode: PermissionMode,
    pub placed: TeammateMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub log: Option<PathBuf>,
    pub spec: (PathBuf, domain::team::TeammateSpec),
}

const OSASCRIPT: &str = "osascript";

fn spec(launching: &Launching<'_>) -> (PathBuf, domain::team::TeammateSpec) {
    let path = launching.team.dir.join("spawn").join(format!("{}.json", file_name(launching.name)));
    let details = domain::team::TeammateSpec {
        team: launching.team.name.clone(),
        name: launching.name.to_owned(),
        cwd: launching.cwd.display().to_string(),
        model: launching.model.to_owned(),
        mode: domain::command::mode_label(launching.mode).to_owned(),
        instructions: launching.instructions.to_owned(),
    };
    (path, details)
}

fn in_iterm2(spec: (PathBuf, domain::team::TeammateSpec), program: &Path) -> Result<Planned, String> {
    let shown =
        |path: &Path| path.to_str().filter(|path| domain::team::safe_in_a_terminal_command(path)).map(str::to_owned);
    let (Some(program), Some(file)) = (shown(program), shown(&spec.0)) else {
        return Err(format!(
            "an iTerm2 pane runs the teammate through a command line, which takes only paths of letters, digits and \
             /._- ({} or {} has others); set teammateMode to tmux or process",
            program.display(),
            spec.0.display()
        ));
    };
    let script = format!(
        "tell application \"iTerm2\"\n  tell current session of current window\n    split vertically with default \
         profile command \"{program} {SUBCOMMAND} --spec {file}\"\n  end tell\nend tell"
    );
    Ok(Planned { program: PathBuf::from(OSASCRIPT), args: vec!["-e".into(), script.into()], log: None, spec })
}

fn teammate_args(spec: &Path) -> Vec<OsString> {
    vec![SUBCOMMAND.into(), "--spec".into(), spec.as_os_str().to_owned()]
}

pub fn planned(launching: &Launching<'_>, program: PathBuf) -> Result<Planned, String> {
    let spec = spec(launching);
    if launching.placed == TeammateMode::Iterm2 {
        return in_iterm2(spec, &program);
    }
    if launching.placed == TeammateMode::Tmux {
        let mut args: Vec<OsString> =
            vec!["split-window".into(), "-d".into(), "-h".into(), "-c".into(), launching.cwd.as_os_str().to_owned()];
        args.push(program.into_os_string());
        args.extend(teammate_args(&spec.0));
        return Ok(Planned { program: PathBuf::from(TMUX), args, log: None, spec });
    }
    Ok(Planned {
        program,
        args: teammate_args(&spec.0),
        log: Some(launching.team.dir.join("logs").join(format!("{}.log", file_name(launching.name)))),
        spec,
    })
}

fn program() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|error| format!("the app's own program cannot be found: {error}"))
}

pub fn terminal() -> domain::team::Terminal {
    domain::team::Terminal {
        inside_tmux: std::env::var_os("TMUX").is_some_and(|session| !session.is_empty()),
        in_iterm2: std::env::var("TERM_PROGRAM").is_ok_and(|program| program == "iTerm.app"),
    }
}

pub fn launch(launching: &Launching<'_>) -> Result<(), String> {
    launching.team.send(launching.name, LEAD, launching.prompt, None)?;
    let planned = planned(launching, program()?)?;
    let (path, spec) = &planned.spec;
    platform::store::Document::at(path).write(spec).map_err(|error| error.to_string())?;
    let mut command = Command::new(&planned.program);
    command
        .args(&planned.args)
        .current_dir(launching.cwd)
        .stdin(Stdio::null());
    let Some(log) = &planned.log else {
        let opener = planned.program.display();
        let placed = command.output().map_err(|error| format!("{opener} could not be started: {error}"))?;
        if !placed.status.success() {
            return Err(format!(
                "{opener} could not open a pane for the teammate: {}",
                String::from_utf8_lossy(&placed.stderr).trim()
            ));
        }
        return Ok(());
    };
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

    fn launching(team: &Team, placed: TeammateMode) -> Launching<'_> {
        Launching {
            team,
            name: "tester",
            cwd: Path::new("/work"),
            prompt: "fix the tests",
            instructions: "# Team coordination",
            model: "claude-sonnet-5-5",
            mode: PermissionMode::AcceptEdits,
            placed,
        }
    }

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn a_teammate_runs_the_app_s_own_program_in_its_team_with_its_name_mode_and_instructions() {
        let team = Team::named(Path::new("/home/me"), "team-1");

        let planned = planned(&launching(&team, TeammateMode::Process), PathBuf::from("/bin/app")).expect("planned");

        let args = strings(&planned.args);
        let spec_file = format!("/home/me/{}/teams/team-1/spawn/tester.json", brand::DIR);
        assert_eq!(args, ["teammate", "--spec", spec_file.as_str()]);
        let (path, spec) = &planned.spec;
        assert_eq!(path, &PathBuf::from(&spec_file));
        assert_eq!((spec.team.as_str(), spec.name.as_str(), spec.cwd.as_str()), ("team-1", "tester", "/work"));
        assert_eq!((spec.model.as_str(), spec.instructions.as_str()), ("claude-sonnet-5-5", "# Team coordination"));
        assert_eq!(spec.mode, domain::command::mode_label(PermissionMode::AcceptEdits));
        assert!(planned.log.is_some_and(|log| log.ends_with("teams/team-1/logs/tester.log")));
        assert!(!args.iter().any(|arg| arg.contains("fix the tests")), "the task goes through the mailbox");
    }

    #[test]
    fn in_tmux_a_teammate_opens_in_a_pane_beside_the_lead_with_no_shell_in_between() {
        let team = Team::named(Path::new("/home/me"), "team-1");

        let planned = planned(&launching(&team, TeammateMode::Tmux), PathBuf::from("/bin/app")).expect("planned");

        assert_eq!(planned.program, PathBuf::from("tmux"));
        let args = strings(&planned.args);
        assert_eq!(args[..5], ["split-window", "-d", "-h", "-c", "/work"]);
        let at = args.iter().position(|arg| arg == "/bin/app").expect("the program");
        assert_eq!(args.get(at + 1).map(String::as_str), Some("teammate"));
        assert_eq!(args[at + 1..].len(), 3, "each of the app's arguments stays its own argument");
        assert!(!args.iter().any(|arg| arg.contains('=')), "nothing goes through the environment");
        assert_eq!(planned.log, None, "the pane shows what it prints");
    }

    #[test]
    fn in_iterm2_a_pane_runs_the_teammate_from_a_spec_file_and_unsafe_paths_are_refused() {
        let team = Team::named(Path::new("/home/me"), "team-1");

        let planned = planned(&launching(&team, TeammateMode::Iterm2), PathBuf::from("/bin/app")).expect("planned");

        assert_eq!(planned.program, PathBuf::from("osascript"));
        let script = strings(&planned.args).join(" ");
        assert!(script.contains("split vertically"), "{script}");
        let spec_file = format!("/home/me/{}/teams/team-1/spawn/tester.json", brand::DIR);
        assert!(script.contains(&format!("command \"/bin/app teammate --spec {spec_file}\"")), "{script}");
        assert!(!script.contains("Team coordination") && !script.contains("fix the tests"), "nothing else is typed");
        let (path, spec) = planned.spec;
        assert!(path.ends_with("spawn/tester.json"));
        assert_eq!((spec.team.as_str(), spec.name.as_str(), spec.cwd.as_str()), ("team-1", "tester", "/work"));
        assert_eq!(spec.instructions, "# Team coordination");
        let spaced = super::planned(&launching(&team, TeammateMode::Iterm2), PathBuf::from("/My Apps/app"));
        assert!(spaced.is_err_and(|error| error.contains("tmux or process")));
    }
}
