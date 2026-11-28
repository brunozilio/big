use app::labels;
use app::policy_gate::{self, Asked};
use connectors::{ActionError, ActionSpec, Labelled};
use domain::audit::code;
use domain::label::{Held, Levels, Mark, Read, Sink, Trust, envelope};
use domain::permission::{Layer, Reason, Verdict};

use super::session_reads::{self, Session, held};

const LOST: &str = "a session this server no longer knows";

fn sessionless(asked: Asked<'_>, tool: &str) -> String {
    let told = format!(
        "{tool} reaches a connection, and with enforced sensitivity levels that happens only inside an MCP session; send initialize and then the session id."
    );
    policy_gate::blocked(asked, tool, &told, Layer::Declared);
    policy_gate::refusal(&told)
}

fn refused(asked: Asked<'_>, tool: &str, spec: &ActionSpec, levels: &Levels, reads: &[Read]) -> Option<String> {
    let mark = Mark::of(levels, reads);
    let sink = Sink { ceiling: spec.max_input_level.as_deref(), risky: spec.risky() };
    let verdict = match labels::flow(asked, tool, levels, &mark, &sink) {
        Verdict::Allow => labels::paired(asked, levels, &Held::of(levels, reads), &spec.next(tool)),
        crossed => crossed,
    };
    match verdict {
        Verdict::Ask { reason: reason @ Reason::Two(_), .. } => {
            Some(policy_gate::unattended(asked, tool, &reason, code::TWO_UNATTENDED))
        }
        Verdict::Ask { reason, .. } => Some(policy_gate::unattended(asked, tool, &reason, code::FLOW_UNATTENDED)),
        Verdict::Deny(reason) => Some(policy_gate::refusal(&reason)),
        Verdict::Allow => None,
    }
}

fn shown(levels: &Levels, labelled: Labelled) -> (Read, String) {
    let read = labels::classified(levels, labelled.read, &labelled.value);
    let text = match labelled.value {
        serde_json::Value::String(text) => text,
        value => serde_json::to_string_pretty(&value).unwrap_or_default(),
    };
    let wrapped = envelope(&read, &text);
    (read, wrapped)
}

pub struct Guarded<'a> {
    pub asked: Asked<'a>,
    pub session: Option<&'a str>,
    pub tool: &'a str,
    pub spec: &'a ActionSpec,
    pub levels: &'a Levels,
}

impl Guarded<'_> {
    pub fn run(&self, call: impl FnOnce() -> Result<Labelled, ActionError>) -> Result<String, String> {
        let found = session_reads::find(self.session);
        let enforced = self.levels.enforced() || self.levels.pairing_enforced();
        let reads = match &found {
            Session::Sessionless if enforced => return Err(sessionless(self.asked, self.tool)),
            Session::Open(reads) => Some(reads),
            Session::Lost | Session::Sessionless => None,
        };
        let mut hold = reads.filter(|_| enforced).map(|reads| held(reads));
        let mut known = match (&hold, reads) {
            (Some(guard), _) => guard.to_vec(),
            (None, Some(reads)) => held(reads).clone(),
            (None, None) => Vec::new(),
        };
        if matches!(found, Session::Lost) && enforced {
            known.push(Read::new(LOST, Trust::External, None));
        }
        if let Some(refusal) = refused(self.asked, self.tool, self.spec, self.levels, &known) {
            return Err(refusal);
        }
        let (read, text) = shown(self.levels, call().map_err(|error| error.message)?);
        match (hold.as_mut(), reads) {
            (Some(guard), _) => kept(guard, read),
            (None, Some(reads)) => kept(&mut held(reads), read),
            (None, None) => {}
        }
        Ok(text)
    }
}

fn kept(reads: &mut Vec<Read>, read: Read) {
    if !reads.contains(&read) {
        reads.push(read);
    }
}

#[cfg(test)]
#[path = "connector_flow_tests.rs"]
mod tests;
