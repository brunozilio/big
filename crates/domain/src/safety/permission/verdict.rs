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
    Declared(String),
    Outside(PathBuf),
    Guarded { path: PathBuf, guard: Guard },
    Hard(super::hard::Category),
}

impl Reason {
    pub fn rememberable(&self) -> bool {
        self.reviewable()
    }

    pub fn reviewable(&self) -> bool {
        !matches!(self, Self::Hook | Self::Listed | Self::Flagged(_) | Self::Hard(_) | Self::Declared(_))
    }

    pub fn unattended(&self, tool: &str) -> String {
        match self {
            Self::Flagged(why) => format!("{tool} was blocked. {why} Find a safer way to do this or ask the user."),
            Self::Hard(category) => {
                format!(
                    "{tool} was blocked because {}. The safety floor has a person approve that every time, and this run has no one to ask, so it was refused. Find a safer way to do this or ask the user.",
                    category.why()
                )
            }
            Self::Declared(why) => {
                format!("{tool} {why}, so a person approves it every time; it does not run unattended.")
            }
            _ => format!("{tool} requires permission; dontAsk denies unapproved actions."),
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

    pub fn without_the_user(self, tool: &str) -> Self {
        match self {
            Self::Ask { reason, .. } => Self::Deny(reason.unattended(tool)),
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
