use std::path::Path;

use super::{Call, Effect, Guard, Judgement, PolicyContext, Reason, Remembered, Standing, Verdict};
use crate::PermissionMode;

#[derive(Debug, Clone, Copy)]
pub struct PolicyEngine;

struct Stood {
    standing: Standing,
    subject: String,
    guarded: Option<(std::path::PathBuf, Guard)>,
}

impl PolicyEngine {
    pub fn decide(call: &Call<'_>, cx: &PolicyContext<'_>) -> Verdict {
        Self::judge(call, cx).verdict
    }

    pub fn judge(call: &Call<'_>, cx: &PolicyContext<'_>) -> Judgement {
        let stood = standing(call, cx);
        let planned = cx.mode() == PermissionMode::Plan && only_commands(call);
        let decided = by_mode(cx.mode(), stood.standing, call.tool, &stood.subject, cx.remembered);
        let verdict = match if planned { Verdict::Allow } else { decided } {
            Verdict::Ask { .. } => {
                let reason = match (&stood.guarded, stood.standing) {
                    (Some((path, guard)), _) => Reason::Guarded { path: path.clone(), guard: *guard },
                    (None, Standing::ReadsOutside) => Reason::Outside(stood.subject.clone().into()),
                    (None, _) => Reason::Mode,
                };
                Verdict::Ask { rememberable: reason.rememberable(), reason }
            }
            settled => settled,
        };
        Judgement { verdict, standing: stood.standing, subject: stood.subject, guarded: stood.guarded }
    }
}

fn only_commands(call: &Call<'_>) -> bool {
    let runs = call.effects.iter().any(|effect| matches!(effect, Effect::Exec(_)));
    let only = call.effects.iter().all(|effect| matches!(effect, Effect::Exec(_)) || !(effect.acts() || effect.changes_files()));
    runs && only
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

fn standing(call: &Call<'_>, cx: &PolicyContext<'_>) -> Stood {
    let outside = call.effects.iter().find_map(|effect| match effect {
        Effect::ReadPath(place) | Effect::ReadTree(place) if !cx.bounds.may_read(&place.real) => Some(place),
        _ => None,
    });
    let guarded = call.effects.iter().find_map(|effect| match effect {
        Effect::WritePath(place) => cx.bounds.guard(&place.real).map(|guard| (place.real.clone(), guard)),
        _ => None,
    });
    let plain = |standing| Stood { standing, subject: call.subject.to_owned(), guarded: None };
    if call.effects.iter().any(Effect::acts) {
        return plain(Standing::Acts);
    }
    match (outside, guarded) {
        (Some(place), _) => Stood { standing: Standing::ReadsOutside, subject: shown(&place.written), guarded: None },
        (None, Some((path, guard))) => {
            Stood { standing: Standing::EditsGuarded, subject: shown(&path), guarded: Some((path, guard)) }
        }
        (None, None) if call.effects.iter().any(Effect::changes_files) => plain(Standing::Edits),
        (None, None) => plain(Standing::Reads),
    }
}

fn by_mode(mode: PermissionMode, standing: Standing, tool: &str, subject: &str, remembered: &Remembered) -> Verdict {
    let ask = || Verdict::Ask { reason: Reason::Mode, rememberable: true };
    match (mode, standing) {
        (_, Standing::Reads) | (PermissionMode::Bypass, _) => Verdict::Allow,
        (_, Standing::ReadsOutside) if remembered.allows(tool, standing, subject) => Verdict::Allow,
        (_, Standing::ReadsOutside) => ask(),
        (PermissionMode::Plan, _) => Verdict::Deny(format!(
            "{tool} is not available while planning: only read and search now, then present the plan."
        )),
        (PermissionMode::AcceptEdits, Standing::Edits) => Verdict::Allow,
        _ if remembered.allows(tool, standing, subject) => Verdict::Allow,
        _ => ask(),
    }
}
