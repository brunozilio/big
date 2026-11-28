#![allow(unsafe_code)]

use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::confine::Ceiling;

pub fn may_confine() -> Result<(), String> {
    if unsafe { libc::geteuid() } == 0 {
        return Ok(());
    }
    Err("agents run as their own users only when the server runs as root".to_owned())
}

pub fn narrow_umask() {
    unsafe { libc::umask(0o077) };
}

pub fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

pub fn open_entry(path: &Path) -> io::Result<()> {
    let found = std::fs::symlink_metadata(path)?;
    if found.file_type().is_symlink() {
        return Ok(());
    }
    let mode = found.permissions().mode();
    let wanted = if found.is_dir() || mode & 0o100 != 0 { mode | 0o005 } else { mode | 0o004 };
    if wanted == mode {
        return Ok(());
    }
    set_mode(path, wanted)
}

pub fn passable(path: &Path) -> io::Result<()> {
    let mode = std::fs::metadata(path)?.permissions().mode();
    if mode & 0o001 != 0 {
        return Ok(());
    }
    set_mode(path, mode | 0o001)
}

pub fn owner(path: &Path) -> io::Result<u32> {
    Ok(std::fs::symlink_metadata(path)?.uid())
}

pub fn give(path: &Path, uid: u32) -> io::Result<()> {
    std::os::unix::fs::lchown(path, Some(uid), Some(uid))
}

pub fn give_file(file: &std::fs::File, uid: u32) -> io::Result<()> {
    std::os::unix::fs::fchown(file, Some(uid), Some(uid))
}

pub fn switch_to(command: &mut Command, uid: u32, ceiling: Ceiling) {
    command.uid(uid).gid(uid);
    unsafe {
        command.pre_exec(move || {
            libc::umask(0o077);
            capped(ceiling)
        });
    }
}

fn limit(value: u64) -> libc::rlimit {
    libc::rlimit { rlim_cur: value as libc::rlim_t, rlim_max: value as libc::rlim_t }
}

fn capped(ceiling: Ceiling) -> io::Result<()> {
    if ceiling.cpu_seconds > 0 && unsafe { libc::setrlimit(libc::RLIMIT_CPU, &limit(ceiling.cpu_seconds)) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if ceiling.memory_bytes > 0 && unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit(ceiling.memory_bytes)) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if ceiling.processes > 0 && unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &limit(ceiling.processes)) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if ceiling.file_bytes > 0 && unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &limit(ceiling.file_bytes)) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
