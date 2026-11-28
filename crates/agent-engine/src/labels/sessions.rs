use std::sync::{Mutex, PoisonError};

use domain::label::SessionReads;

pub const MOST: usize = 256;

static SESSIONS: Mutex<Vec<(String, SessionReads)>> = Mutex::new(Vec::new());

pub fn exposed(session: &str) -> SessionReads {
    let mut kept = SESSIONS.lock().unwrap_or_else(PoisonError::into_inner);
    let found = kept.iter().position(|(known, _)| known == session);
    let entry = match found {
        Some(at) => kept.remove(at),
        None => (session.to_owned(), SessionReads::default()),
    };
    let reads = entry.1.clone();
    kept.push(entry);
    let over = kept.len().saturating_sub(MOST);
    kept.drain(..over);
    reads
}

#[cfg(test)]
mod tests {
    use domain::label::{Read, Trust};

    use super::*;

    #[test]
    fn a_session_keeps_what_its_tools_brought_across_runs_until_it_is_pushed_out() {
        exposed("s-turns").keep(Read::new("issue", Trust::External, None));
        assert_eq!(exposed("s-turns").held().len(), 1, "the next turn sees the first");
        assert!(exposed("s-other").held().is_empty());
        for n in 0..MOST {
            exposed(&format!("s-many-{n}"));
        }
        assert!(exposed("s-turns").held().is_empty(), "the least recent leaves first");
    }
}
