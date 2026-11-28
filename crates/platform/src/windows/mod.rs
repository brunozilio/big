mod private;
mod shell;
mod tree;

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

pub use shell::WindowsShell;
pub use tree::alive;
pub(crate) use tree::{Tree, lost};

pub const NAME: &str = "Windows";
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn release_target() -> Option<&'static str> {
    None
}

pub fn cpu_time() -> Option<std::time::Duration> {
    None
}

pub fn port_holder(_port: u16) -> Option<(u32, String)> {
    None
}

pub fn terminate(_pid: u32) -> std::io::Result<()> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

pub fn shell(code: &str) -> Command {
    WindowsShell::detect().command(code)
}

pub fn script_shell(code: &str) -> Command {
    WindowsShell::detect().command(code)
}

pub fn shell_name() -> String {
    WindowsShell::detect().name().to_owned()
}

pub fn restrict_open(_options: &mut OpenOptions) {}

pub fn restrict(_file: &File, path: &Path) -> std::io::Result<()> {
    private::restrict(path)
}

pub fn keep_owner(_file: &File, _path: &Path) -> std::io::Result<()> {
    Ok(())
}

pub fn open_with_mode_of(options: &mut OpenOptions, path: &Path, _existing: &Path) -> std::io::Result<File> {
    options.open(path)
}

pub fn is_file_at(_file: &File, _path: &Path) -> bool {
    true
}

pub const LOCK_FILES_CAN_BE_REMOVED: bool = false;

pub fn make_executable(_file: &File) -> std::io::Result<()> {
    Ok(())
}

pub fn make_private_dir(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

pub fn nonblocking(_options: &mut OpenOptions) {}

pub fn open_url(url: &str) -> Command {
    let mut open = command("explorer");
    open.arg(url);
    open
}

pub fn clipboard_image() -> Option<Vec<u8>> {
    None
}

pub fn data_dir_in(
    var: impl Fn(&str) -> Option<OsString>,
    home: Option<PathBuf>,
    name: &str,
    _command: &str,
) -> Option<PathBuf> {
    var("APPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join("AppData\\Roaming")))
        .map(|dir| dir.join(name))
}

pub fn local_data_dir_in(
    var: impl Fn(&str) -> Option<OsString>,
    home: Option<PathBuf>,
    name: &str,
    _command: &str,
) -> Option<PathBuf> {
    var("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join("AppData\\Local")))
        .map(|dir| dir.join(name))
}

pub fn may_confine() -> Result<(), String> {
    Err("agents run as their own users only on Linux and macOS".to_owned())
}

pub fn narrow_umask() {}

pub fn set_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

pub fn open_entry(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

pub fn passable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

pub fn owner(_path: &Path) -> std::io::Result<u32> {
    Err(std::io::Error::other("files have no numeric owner here"))
}

pub fn give(_path: &Path, _uid: u32) -> std::io::Result<()> {
    Ok(())
}

pub fn give_file(_file: &File, _uid: u32) -> std::io::Result<()> {
    Ok(())
}

pub fn switch_to(_command: &mut Command, _uid: u32, _ceiling: crate::confine::Ceiling) {}
