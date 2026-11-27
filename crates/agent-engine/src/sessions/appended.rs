use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::Path;

use super::log::{self, Entry};

const CHECKED: usize = 64;

#[derive(Debug, Default)]
pub(super) struct Parsed {
    bytes: u64,
    check: Vec<u8>,
    entries: Vec<Entry>,
}

#[derive(Debug)]
pub(super) struct Reread {
    pub(super) parsed: Parsed,
    pub(super) entries: Vec<Entry>,
    pub(super) skipped: Vec<String>,
}

fn failed(error: std::io::Error) -> String {
    format!("the saved conversation could not be read: {error}")
}

pub(super) fn read(path: &Path, kept: Option<Parsed>) -> Result<Option<Reread>, String> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failed(error)),
    };
    let length = file.metadata().map_err(failed)?.len();
    let mut parsed = kept.filter(|kept| unchanged_before(&mut file, kept, length)).unwrap_or_default();
    file.seek(SeekFrom::Start(parsed.bytes)).map_err(failed)?;
    let mut fresh = Vec::new();
    file.read_to_end(&mut fresh).map_err(failed)?;
    let whole = fresh.iter().rposition(|byte| *byte == b'\n').map_or(0, |at| at + 1);
    let (said, mut skipped) = log::lines(&String::from_utf8_lossy(&fresh[..whole]));
    parsed.entries.extend(said);
    parsed.bytes += whole as u64;
    let mut check = std::mem::take(&mut parsed.check);
    check.extend_from_slice(&fresh[..whole]);
    parsed.check = check.split_off(check.len().saturating_sub(CHECKED));
    let (unfinished, cut) = log::lines(&String::from_utf8_lossy(&fresh[whole..]));
    skipped.extend(cut);
    let mut entries = parsed.entries.clone();
    entries.extend(unfinished);
    Ok(Some(Reread { parsed, entries, skipped }))
}

fn unchanged_before(file: &mut File, kept: &Parsed, length: u64) -> bool {
    if kept.bytes > length {
        return false;
    }
    let start = kept.bytes - kept.check.len() as u64;
    let mut seen = vec![0; kept.check.len()];
    file.seek(SeekFrom::Start(start)).is_ok() && file.read_exact(&mut seen).is_ok() && seen == kept.check
}
