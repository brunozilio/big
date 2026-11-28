use domain::audit::code;
use domain::label::{Held, Levels, Next};
use domain::permission::{Reason, Verdict};

use crate::policy_gate::Asked;

pub fn paired(asked: Asked<'_>, levels: &Levels, held: &Held, next: &Next<'_>, approved: bool) -> Verdict {
    let Some(why) = held.joined(next) else { return Verdict::Allow };
    let enforced = levels.pairing_enforced();
    if enforced && approved {
        standing(asked, next.tool, &why);
        return Verdict::Allow;
    }
    if enforced || !held.all() {
        told(asked, levels, &held.with(next), next.tool, &why);
    }
    if enforced { Verdict::Ask { reason: Reason::Two(why), rememberable: false } } else { Verdict::Allow }
}

fn standing(asked: Asked<'_>, tool: &str, why: &str) {
    tracing::info!(
        target: "audit",
        surface = asked.surface.word(),
        stage = asked.stage,
        user = asked.owner,
        tool,
        reason = why,
        mode = domain::label::Mode::Enforce.word(),
        decision.code = code::TWO_SESSION,
        layer = domain::permission::Layer::Declared.word(),
        "two.allowed"
    );
}

fn told(asked: Asked<'_>, levels: &Levels, joined: &Held, tool: &str, why: &str) {
    let enforced = levels.pairing_enforced();
    tracing::warn!(
        target: "audit",
        surface = asked.surface.word(),
        stage = asked.stage,
        user = asked.owner,
        tool,
        reason = why,
        untrusted = joined.untrusted.as_deref().unwrap_or_default(),
        sensitive = joined.sensitive.as_deref().unwrap_or_default(),
        acted = joined.acted.as_deref().unwrap_or_default(),
        mode = levels.paired().word(),
        decision.code = if enforced { code::TWO_ASKED } else { code::TWO_SHADOW },
        layer = domain::permission::Layer::Declared.word(),
        "two.crossed"
    );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use domain::label::{Mode, Read, Trust};
    use domain::permission::Surface;

    use super::*;

    const ASKED: Asked<'static> = Asked { surface: Surface::Chat, stage: "s", owner: "local", operation: None };

    #[derive(Default, Clone)]
    struct Crossings(Arc<AtomicUsize>, Arc<AtomicUsize>);

    #[derive(Default)]
    struct Message(Option<String>);

    impl tracing::field::Visit for Message {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0 = Some(format!("{value:?}"));
            }
        }
    }

    impl tracing::Subscriber for Crossings {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            let mut seen = Message::default();
            event.record(&mut seen);
            match seen.0.as_deref() {
                Some("two.crossed") => drop(self.0.fetch_add(1, Ordering::SeqCst)),
                Some("two.allowed") => drop(self.1.fetch_add(1, Ordering::SeqCst)),
                _ => {}
            }
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    fn crossings(mode: Mode, approved_after: usize) -> (usize, usize, usize) {
        let levels = Levels { mode, ..Levels::default() };
        let mut reads = vec![
            Read::new("issue", Trust::External, None),
            Read::new("vault", Trust::System, None).exposed(true, false),
        ];
        let comment = Next { tool: "comment", acts: true, ..Next::default() };
        let heard = Crossings::default();
        let mut asked = 0;
        tracing::subscriber::with_default(heard.clone(), || {
            for call in 0..11 {
                let approved = call >= approved_after;
                if paired(ASKED, &levels, &Held::of(&levels, &reads), &comment, approved).asks() {
                    asked += 1;
                }
                reads.push(Read::new("comment", Trust::System, None).exposed(false, true));
            }
        });
        (heard.0.load(Ordering::SeqCst), heard.1.load(Ordering::SeqCst), asked)
    }

    #[test]
    fn in_shadow_only_the_call_that_completes_the_three_is_audited_and_in_enforce_every_ask_is() {
        assert_eq!(crossings(Mode::Shadow, 11), (1, 0, 0), "ten calls after the crossing add nothing");
        assert_eq!(crossings(Mode::Enforce, 11), (11, 0, 11));
    }

    #[test]
    fn once_the_session_said_yes_the_next_crossings_run_and_each_is_still_audited() {
        assert_eq!(crossings(Mode::Enforce, 1), (1, 10, 1), "one ask, then ten audited without asking");
        assert_eq!(crossings(Mode::Shadow, 1), (1, 0, 0), "a yes changes nothing in shadow");
    }
}
