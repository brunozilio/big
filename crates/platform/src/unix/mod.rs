mod ports;
mod private;
mod tree;

use std::fs::OpenOptions;
use std::path::Path;
use std::process::Command;

pub use ports::{port_holder, terminate};
pub use private::{LOCK_FILES_CAN_BE_REMOVED, is_file_at, keep_owner, open_with_mode_of, restrict, restrict_open};
pub use tree::alive;
pub(crate) use tree::{Tree, lost};

pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    Command::new(program)
}

const BASH: &[&str] = &["/opt/homebrew/bin/bash", "/usr/local/bin/bash", "/bin/bash", "/usr/bin/bash"];

fn program_name(program: &str) -> String {
    Path::new(program).file_name().map_or_else(|| program.to_owned(), |name| name.to_string_lossy().into_owned())
}

fn shell_program_in(user: Option<String>, exists: impl Fn(&str) -> bool) -> String {
    let user = user.filter(|shell| !shell.is_empty());
    if let Some(user) = user.as_deref().filter(|shell| program_name(shell) == "bash") {
        return user.to_owned();
    }
    let bash = BASH.iter().find(|bash| exists(bash)).map(|bash| (*bash).to_owned());
    bash.or(user).unwrap_or_else(|| "/bin/sh".to_owned())
}

fn shell_program() -> String {
    shell_program_in(std::env::var("SHELL").ok(), |path| Path::new(path).is_file())
}

pub fn shell(code: &str) -> Command {
    let mut shell = command(shell_program());
    shell.arg("-c").arg(code);
    shell
}

pub fn script_shell(code: &str) -> Command {
    let mut shell = command("/bin/sh");
    shell.args(["-c", code]);
    shell
}

pub fn shell_name() -> String {
    program_name(&shell_program())
}

pub fn make_executable(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o755))
}

pub fn make_private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

pub fn nonblocking(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NONBLOCK);
}

pub fn capture(program: &str, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new(program).args(args).output().ok()?;
    (output.status.success() && !output.stdout.is_empty()).then_some(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_run_with_bash_when_there_is_one_and_otherwise_with_the_user_s_shell() {
        let none = |_: &str| false;
        let system = |path: &str| path == "/bin/bash";
        assert_eq!(shell_program_in(Some("/bin/zsh".into()), system), "/bin/bash");
        assert_eq!(shell_program_in(Some("/opt/bin/bash".into()), system), "/opt/bin/bash");
        assert_eq!(shell_program_in(Some("/bin/zsh".into()), none), "/bin/zsh");
        assert_eq!(shell_program_in(Some(String::new()), none), "/bin/sh");
        assert_eq!(shell_program_in(None, system), "/bin/bash");
        assert_eq!(program_name("/opt/homebrew/bin/bash"), "bash");
    }
}
