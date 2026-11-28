use domain::audit::code;
use domain::label::{Held, Levels, Next};
use domain::permission::{Reason, Verdict};

use crate::policy_gate::Asked;

pub fn paired(asked: Asked<'_>, levels: &Levels, held: &Held, next: &Next<'_>) -> Verdict {
    let Some(why) = held.joined(next) else { return Verdict::Allow };
    let enforced = levels.pairing_enforced();
    let joined = held.with(next);
    tracing::warn!(
        target: "audit",
        surface = asked.surface.word(),
        stage = asked.stage,
        user = asked.owner,
        tool = next.tool,
        reason = why.as_str(),
        untrusted = joined.untrusted.as_deref().unwrap_or_default(),
        sensitive = joined.sensitive.as_deref().unwrap_or_default(),
        acted = joined.acted.as_deref().unwrap_or_default(),
        mode = levels.paired().word(),
        decision.code = if enforced { code::TWO_ASKED } else { code::TWO_SHADOW },
        layer = domain::permission::Layer::Declared.word(),
        "two.crossed"
    );
    if enforced { Verdict::Ask { reason: Reason::Two(why), rememberable: false } } else { Verdict::Allow }
}
