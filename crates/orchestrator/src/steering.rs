use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::asks::SharedRun;

#[derive(Default)]
pub struct Steering {
    runs: Mutex<HashMap<String, SharedRun>>,
    clients: Mutex<HashMap<String, u64>>,
}

impl fmt::Debug for Steering {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Steering").field("runs", &self.held().len()).finish()
    }
}

impl Steering {
    fn held(&self) -> MutexGuard<'_, HashMap<String, SharedRun>> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn follow(&self, session: &str, run: &SharedRun) {
        self.held().insert(session.to_owned(), Arc::clone(run));
    }

    pub fn release(&self, run: &SharedRun) {
        self.held().retain(|_, known| !Arc::ptr_eq(known, run));
    }

    fn owners(&self) -> MutexGuard<'_, HashMap<String, u64>> {
        self.clients.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn own(&self, session: &str, client: u64) {
        self.owners().insert(session.to_owned(), client);
    }

    pub fn disown(&self, client: u64) {
        self.owners().retain(|_, known| *known != client);
    }

    pub fn client(&self, session: &str) -> Option<u64> {
        self.owners().get(session).copied()
    }

    pub fn steer(&self, session: &str, text: &str) -> bool {
        let Some(run) = self.held().get(session).cloned() else { return false };
        run.lock().unwrap_or_else(PoisonError::into_inner).steer(text.to_owned());
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use agent_engine::RunHandle;

    use super::Steering;
    use crate::asks::SharedRun;

    struct Heard(Arc<Mutex<Vec<String>>>);

    impl RunHandle for Heard {
        fn cancel(&mut self) {}
        fn steer(&mut self, text: String) {
            self.0.lock().expect("heard").push(text);
        }
    }

    fn run() -> (SharedRun, Arc<Mutex<Vec<String>>>) {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let run: SharedRun = Arc::new(Mutex::new(Box::new(Heard(Arc::clone(&heard)))));
        (run, heard)
    }

    #[test]
    fn a_message_reaches_the_run_of_its_session_only_while_it_runs() {
        let steering = Steering::default();
        let (run, heard) = run();
        assert!(!steering.steer("s1", "before"));
        steering.follow("s1", &run);
        assert!(steering.steer("s1", "also the tests"));
        assert!(!steering.steer("s2", "elsewhere"));
        steering.release(&run);
        assert!(!steering.steer("s1", "after"));
        assert_eq!(*heard.lock().expect("heard"), ["also the tests"]);
    }

    #[test]
    fn a_step_s_session_knows_its_client_until_the_step_lets_it_go() {
        let steering = Steering::default();
        assert_eq!(steering.client("s1"), None);
        steering.own("s1", 7);
        steering.own("s2", 8);
        assert_eq!(steering.client("s1"), Some(7));
        steering.disown(7);
        assert_eq!((steering.client("s1"), steering.client("s2")), (None, Some(8)));
    }
}
