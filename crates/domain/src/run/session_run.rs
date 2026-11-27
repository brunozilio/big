use crate::budget::Spending;
use crate::usage::SessionCommand;
use crate::{EngineEvent, PermissionMode, SessionId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Touches {
    pub session: bool,
    pub conversation: bool,
}

impl Touches {
    pub fn of(command: Option<SessionCommand>) -> Self {
        Self { session: !command.is_some_and(SessionCommand::is_chore), conversation: command.is_none() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunWatch {
    touches: Touches,
}

impl RunWatch {
    pub fn of(command: Option<SessionCommand>) -> Self {
        Self { touches: Touches::of(command) }
    }

    pub fn touches(&self) -> Touches {
        self.touches
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Carried {
    pub id: Option<SessionId>,
    pub mode: PermissionMode,
    pub spending: Spending,
    fork_asked: bool,
}

impl Carried {
    pub fn fork_next(&mut self) {
        self.fork_asked = true;
    }

    pub fn forks(&mut self) -> bool {
        self.id.is_some() && std::mem::take(&mut self.fork_asked)
    }

    pub fn observe(&mut self, watch: &RunWatch, event: &EngineEvent) {
        self.spending.observe(event);
        if watch.touches.session
            && let EngineEvent::SessionStarted(id) = event
        {
            self.id = Some(id.clone());
        }
        if !watch.touches.conversation {
            return;
        }
        if let EngineEvent::ModeChanged(mode) = event {
            self.mode = *mode;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DONE: EngineEvent = EngineEvent::Completed { duration_ms: 1, cost_usd: None };

    fn started(id: &str) -> EngineEvent {
        EngineEvent::SessionStarted(SessionId(id.into()))
    }

    fn cost(usd: f64) -> EngineEvent {
        EngineEvent::Usage(crate::billing::ModelUsage { estimated_cost_usd: Some(usd), ..Default::default() })
    }

    fn after(carried: &Carried, command: Option<SessionCommand>, events: &[EngineEvent]) -> Carried {
        let mut carried = carried.clone();
        let watch = RunWatch::of(command);
        for event in events {
            carried.observe(&watch, event);
        }
        carried
    }

    #[test]
    fn a_prompt_is_the_conversation_a_session_command_changes_the_session_and_a_chore_neither() {
        assert_eq!(Touches::of(None), Touches { session: true, conversation: true });
        for command in [SessionCommand::Compact, SessionCommand::Rewind(1), SessionCommand::Branch(1)] {
            assert_eq!(Touches::of(Some(command)), Touches { session: true, conversation: false }, "{command:?}");
        }
        assert!(Touches::of(Some(SessionCommand::MeasureContext)).session);
        for chore in [
            SessionCommand::Title,
            SessionCommand::Suggest,
            SessionCommand::Recap,
            SessionCommand::Side,
            SessionCommand::HeadsUp,
            SessionCommand::Rename,
            SessionCommand::Learn,
            SessionCommand::Permissions,
            SessionCommand::Answer,
        ] {
            assert_eq!(Touches::of(Some(chore)), Touches::default(), "{chore:?}");
        }
        assert_eq!(RunWatch::default().touches(), Touches::default(), "a run of no session touches nothing");
    }

    #[test]
    fn a_prompt_s_run_moves_the_session_its_mode_and_its_spending() {
        let events = [started("s"), EngineEvent::ModeChanged(PermissionMode::Plan), cost(0.5), DONE];
        let carried = after(&Carried::default(), None, &events);

        assert_eq!(carried.id, Some(SessionId("s".into())));
        assert_eq!(carried.mode, PermissionMode::Plan);
        assert!((carried.spending.spent_usd - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn a_chore_only_spends() {
        let resumed = Carried { id: Some(SessionId("s".into())), ..Carried::default() };
        let events = [
            started("another"),
            EngineEvent::ModeChanged(PermissionMode::Auto),
            EngineEvent::Steered("one".into()),
            EngineEvent::Steered("two".into()),
            cost(0.25),
            DONE,
        ];
        let carried = after(&resumed, Some(SessionCommand::Side), &events);

        assert_eq!(carried.id, resumed.id, "the session it was asked about stays the session");
        assert_eq!(carried.mode, resumed.mode);
        assert!((carried.spending.spent_usd - 0.25).abs() < f64::EPSILON, "but its model calls cost");
    }

    #[test]
    fn a_branch_moves_to_its_new_session_without_touching_mode_or_lessons() {
        let resumed = Carried { id: Some(SessionId("s".into())), ..Carried::default() };
        let events = [started("branch"), EngineEvent::ModeChanged(PermissionMode::Auto), DONE];
        let carried = after(&resumed, Some(SessionCommand::Branch(2)), &events);
        assert_eq!(carried.id, Some(SessionId("branch".into())));
        assert_eq!(carried.mode, resumed.mode);
    }

    #[test]
    fn a_fork_is_taken_only_by_a_run_that_resumes_and_only_once() {
        let mut fresh = Carried::default();
        fresh.fork_next();
        assert!(!fresh.forks(), "there is nothing to fork from yet");
        fresh.id = Some(SessionId("s".into()));
        assert!(fresh.forks(), "the request stayed until there was");
        assert!(!fresh.forks(), "once");
        assert!(!Carried { id: Some(SessionId("s".into())), ..Carried::default() }.forks(), "never asked");
    }
}
