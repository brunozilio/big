use domain::label::{Levels, Mark, Sink};
use domain::permission::{Reason, Verdict};

use super::mode;
use crate::policy_gate::Asked;

pub fn flow(asked: Asked<'_>, tool: &str, levels: &Levels, mark: &Mark, sink: &Sink<'_>) -> Verdict {
    let Some(why) = mark.crossing(levels, sink) else { return Verdict::Allow };
    tracing::warn!(
        target: "audit",
        surface = asked.surface.word(),
        stage = asked.stage,
        user = asked.owner,
        tool,
        reason = why.as_str(),
        label = mark.label.as_deref().unwrap_or_default(),
        ceiling = sink.ceiling.unwrap_or_default(),
        mode = mode(levels).word(),
        "label.crossed"
    );
    if levels.enforced() { Verdict::Ask { reason: Reason::Flow(why), rememberable: false } } else { Verdict::Allow }
}
