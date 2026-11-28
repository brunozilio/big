use domain::label::{Held, Levels, Next, Read, SessionReads, Trust};
use domain::operation::Profile;
use domain::permission::{Surface, Verdict};

use crate::agent::Agent;
use crate::policy_gate::Asked;
use crate::tools::{Outcome, Tool};

fn judged(asked: Asked<'_>, levels: &Levels, reads: &[Read], profile: &Profile, tool: &str) -> Verdict {
    let held = Held::of(levels, reads);
    let next = Next { tool, untrusted: profile.untrusted, sensitive: profile.sensitive, acts: profile.acts() };
    crate::labels::paired(asked, levels, &held, &next)
}

fn kept(reads: &SessionReads, tool: &str, profile: &Profile, secret: bool) {
    let trust = if profile.untrusted { Trust::External } else { Trust::System };
    reads.keep(Read::new(tool, trust, None).exposed(profile.sensitive || secret, profile.acts()));
}

impl Agent<'_> {
    pub(in crate::agent) fn paired(&self, tool: &dyn Tool, name: &str, verdict: Verdict) -> Verdict {
        if verdict != Verdict::Allow {
            return verdict;
        }
        let credentials = &self.workspace.credentials;
        let (Some(exposed), Some(profile)) = (&credentials.exposed, tool.profile()) else { return verdict };
        let mut reads = credentials.reads.as_ref().map(SessionReads::held).unwrap_or_default();
        reads.extend(exposed.held());
        let stage = self.workspace.session_id.as_str();
        let owner = self.workspace.owner.as_deref().unwrap_or(domain::principal::LOCAL_USER);
        let asked = Asked { surface: Surface::Chat, stage, owner, operation: None };
        judged(asked, &crate::labels::levels(), &reads, &profile, name)
    }

    pub(in crate::agent) fn noted(&self, tool: &dyn Tool, outcome: &Outcome) {
        let (Some(exposed), Some(profile)) = (&self.workspace.credentials.exposed, tool.profile()) else { return };
        if !outcome.is_error {
            kept(exposed, &tool.spec().name, &profile, self.workspace.policy.holds_a_secret(&outcome.content));
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::label::Mode;
    use domain::operation::{Kind, Reversible, Risk};
    use domain::permission::Reason;

    use super::*;

    const ASKED: Asked<'static> = Asked { surface: Surface::Chat, stage: "s", owner: "local", operation: None };

    fn profile(effects: Vec<Kind>, untrusted: bool, sensitive: bool) -> Profile {
        Profile::new(effects, Risk::Low, Reversible::Yes, untrusted, sensitive)
    }

    #[test]
    fn in_a_chat_the_third_property_asks_only_when_enforced() {
        let issue = profile(vec![Kind::Read], true, false);
        let vault = profile(vec![Kind::Read], false, false);
        let comment = profile(vec![Kind::Write, Kind::SendExternal], false, false);
        for mode in [Mode::Shadow, Mode::Enforce] {
            let levels = Levels { mode, ..Levels::default() };
            let reads = SessionReads::default();
            assert_eq!(judged(ASKED, &levels, &reads.held(), &issue, "issue"), Verdict::Allow);
            kept(&reads, "issue", &issue, false);
            assert_eq!(judged(ASKED, &levels, &reads.held(), &vault, "vault"), Verdict::Allow);
            kept(&reads, "vault", &vault, true);
            let verdict = judged(ASKED, &levels, &reads.held(), &comment, "comment");
            match mode {
                Mode::Shadow => assert_eq!(verdict, Verdict::Allow),
                Mode::Enforce => {
                    let Verdict::Ask { reason: Reason::Two(why), rememberable: false } = verdict else {
                        panic!("the third property asks: {verdict:?}");
                    };
                    assert!(why.contains("from issue") && why.contains("from vault") && why.contains("by comment"));
                }
            }
        }
    }

    #[test]
    fn a_secret_found_in_the_output_or_a_declared_one_marks_the_session_sensitive() {
        let plain = profile(vec![Kind::Read], false, false);
        let reads = SessionReads::default();
        kept(&reads, "notes", &plain, false);
        assert_eq!(Held::of(&Levels::default(), &reads.held()).sensitive, None);
        kept(&reads, "env", &plain, true);
        kept(&reads, "vault", &profile(vec![Kind::Read], false, true), false);
        let held = Held::of(&Levels::default(), &reads.held());
        assert_eq!(held.sensitive.as_deref(), Some("env"));
        assert_eq!(held.acted, None);
    }
}
