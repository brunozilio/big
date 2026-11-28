use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use domain::label::Read;

pub const MOST: usize = 256;

pub type Reads = Arc<Mutex<Vec<Read>>>;

static SESSIONS: Mutex<Vec<(String, Reads)>> = Mutex::new(Vec::new());

#[derive(Debug, Clone)]
pub enum Session {
    Open(Reads),
    Lost,
    Sessionless,
}

pub fn held(reads: &Reads) -> MutexGuard<'_, Vec<Read>> {
    reads.lock().unwrap_or_else(PoisonError::into_inner)
}

fn all() -> MutexGuard<'static, Vec<(String, Reads)>> {
    SESSIONS.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn open(session: &str) {
    let mut kept = all();
    if kept.iter().any(|(known, _)| known == session) {
        return;
    }
    kept.push((session.to_owned(), Reads::default()));
    let over = kept.len().saturating_sub(MOST);
    kept.drain(..over);
}

pub fn close(session: &str) {
    all().retain(|(known, _)| known != session);
}

pub fn find(session: Option<&str>) -> Session {
    let Some(session) = session else { return Session::Sessionless };
    match all().iter().find(|(known, _)| known == session) {
        Some((_, reads)) => Session::Open(Arc::clone(reads)),
        None => Session::Lost,
    }
}

#[cfg(test)]
mod tests {
    use domain::label::Trust;

    use super::*;

    #[test]
    fn a_session_is_known_from_its_opening_until_it_is_closed_or_pushed_out() {
        assert!(matches!(find(None), Session::Sessionless));
        assert!(matches!(find(Some("s-never")), Session::Lost));
        open("s-first");
        let Session::Open(reads) = find(Some("s-first")) else { panic!("opened") };
        held(&reads).push(Read::new("Mail", Trust::External, None));
        open("s-first");
        let Session::Open(again) = find(Some("s-first")) else { panic!("still open") };
        assert_eq!(held(&again).len(), 1, "opening twice keeps what was read");
        for n in 0..MOST {
            open(&format!("s-{n}"));
        }
        assert!(matches!(find(Some("s-first")), Session::Lost), "the oldest leaves first");
        assert!(matches!(find(Some("s-1")), Session::Open(_)));
        close("s-1");
        assert!(matches!(find(Some("s-1")), Session::Lost));
    }
}
