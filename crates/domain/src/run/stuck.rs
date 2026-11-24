use std::collections::BTreeMap;

const FAILURES: u32 = 3;
const HELD_BACK: u32 = 2;
const MOST_ESCALATIONS: u32 = 2;

#[derive(Debug, Default)]
pub struct Stuck {
    failures: BTreeMap<String, u32>,
    held_back: u32,
    escalations: u32,
}

impl Stuck {
    pub fn failed(&mut self, tool: &str, summary: &str) -> bool {
        let count = self.failures.entry(format!("{tool}\u{0}{summary}")).or_default();
        *count += 1;
        *count >= FAILURES && self.escalate()
    }

    pub fn succeeded(&mut self, tool: &str, summary: &str) {
        self.failures.remove(&format!("{tool}\u{0}{summary}"));
    }

    pub fn held_back(&mut self) -> bool {
        self.held_back += 1;
        self.held_back >= HELD_BACK && self.escalate()
    }

    fn escalate(&mut self) -> bool {
        if self.escalations >= MOST_ESCALATIONS {
            return false;
        }
        self.escalations += 1;
        self.failures.clear();
        self.held_back = 0;
        true
    }
}

pub fn nudge(model: &str) -> String {
    format!(
        "\n\nYou have tried this several times without success. The next turns run on {model}: step back, read the \
         failure again and change the approach instead of repeating it."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_call_failing_three_times_is_stuck_once() {
        let mut stuck = Stuck::default();
        assert!(!stuck.failed("Bash", "cargo test"));
        assert!(!stuck.failed("Bash", "cargo test"));
        assert!(!stuck.failed("Bash", "cargo build"));
        assert!(stuck.failed("Bash", "cargo test"));
        assert!(!stuck.failed("Bash", "cargo test"), "the count starts over after help");
    }

    #[test]
    fn a_success_clears_the_failures_of_that_call() {
        let mut stuck = Stuck::default();
        stuck.failed("Bash", "make");
        stuck.failed("Bash", "make");
        stuck.succeeded("Bash", "make");
        assert!(!stuck.failed("Bash", "make"));
    }

    #[test]
    fn held_back_repeats_are_stuck_too_and_help_comes_at_most_twice() {
        let mut stuck = Stuck::default();
        assert!(!stuck.held_back());
        assert!(stuck.held_back());
        assert!(!stuck.held_back());
        assert!(stuck.held_back());
        assert!(!stuck.held_back() && !stuck.held_back(), "a run is helped at most twice");
    }

    #[test]
    fn the_nudge_names_the_stronger_model() {
        assert!(nudge("big").contains("run on big"));
    }
}
