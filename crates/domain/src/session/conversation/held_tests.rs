use super::*;
use crate::EngineEvent;

#[test]
fn a_refused_or_repeated_tool_call_is_part_of_the_conversation() {
    let mut transcript = Transcript::default();
    transcript.begin();
    transcript.apply(&EngineEvent::Refused { tool: "Bash".into(), reason: "rm -rf is denied".into() });
    transcript.apply(&EngineEvent::Repeated { tool: "Read".into() });
    assert_eq!(
        transcript.entries,
        [
            Entry::Refused { tool: "Bash".into(), reason: "rm -rf is denied".into() },
            Entry::Repeated { tool: "Read".into() },
        ]
    );
}
