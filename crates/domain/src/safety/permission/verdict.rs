use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    OutsideTheProject,
    GitFolder,
    OwnConfiguration,
}

impl Guard {
    pub fn why(self) -> &'static str {
        match self {
            Self::OutsideTheProject => "This writes outside the project and the folders you added:",
            Self::GitFolder => {
                "This writes inside the repository's .git folder, where hooks and configuration can run commands:"
            }
            Self::OwnConfiguration => {
                concat!(
                    "This changes ",
                    brand::name!(),
                    "'s own configuration, which can grant permissions and run code:"
                )
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    Mode,
    Hook,
    Listed,
    Flagged(String),
    Declared { why: String, overrode: bool },
    Outside(PathBuf),
    Guarded { path: PathBuf, guard: Guard },
    Hard(super::hard::Category),
    Flow(String),
    Two(String),
}

impl Reason {
    pub fn rememberable(&self) -> bool {
        self.reviewable()
    }

    pub fn reviewable(&self) -> bool {
        !matches!(
            self,
            Self::Hook
                | Self::Listed
                | Self::Flagged(_)
                | Self::Hard(_)
                | Self::Declared { .. }
                | Self::Flow(_)
                | Self::Two(_)
        )
    }

    pub fn shown(&self) -> Option<String> {
        match self {
            Self::Flagged(why) | Self::Flow(why) | Self::Two(why) => Some(why.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Ask { reason: Reason, rememberable: bool },
    Deny(String),
}

impl Verdict {
    pub fn asks(&self) -> bool {
        matches!(self, Self::Ask { .. })
    }

    pub fn denies(&self) -> bool {
        matches!(self, Self::Deny(_))
    }

    pub fn without_the_user(self, told: impl FnOnce(&Reason) -> String) -> Self {
        match self {
            Self::Ask { reason, .. } => Self::Deny(told(&reason)),
            settled => settled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Reads,
    ReadsOutside,
    Edits,
    EditsGuarded,
    Acts,
}

impl Standing {
    pub fn refused_while_planning(self) -> bool {
        matches!(self, Self::Edits | Self::EditsGuarded | Self::Acts)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement {
    pub verdict: Verdict,
    pub standing: Standing,
    pub subject: String,
    pub guarded: Option<(PathBuf, Guard)>,
}

impl Judgement {
    pub fn asking(&self, reason: Reason) -> Verdict {
        Verdict::Ask { rememberable: reason.rememberable(), reason }
    }
}
