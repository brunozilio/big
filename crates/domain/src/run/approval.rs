use serde::{Deserialize, Serialize};

use crate::operation::Disclosed;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AskId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAsk {
    pub id: AskId,
    pub kind: AskKind,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AskKind {
    Permission {
        tool: String,
        subject: String,
        rememberable: bool,
        #[serde(default, skip_serializing_if = "Disclosed::is_empty")]
        disclosed: Disclosed,
    },
    Questions(Vec<Question>),
    Plan {
        plan: String,
    },
}

impl std::fmt::Debug for AskKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Permission { tool, subject, rememberable, disclosed } => {
                let mut shown = f.debug_struct("Permission");
                shown.field("tool", tool).field("subject", subject).field("rememberable", rememberable);
                if !disclosed.is_empty() {
                    shown.field("disclosed", disclosed);
                }
                shown.finish()
            }
            Self::Questions(questions) => f.debug_tuple("Questions").field(questions).finish(),
            Self::Plan { plan } => f.debug_struct("Plan").field("plan", plan).finish(),
        }
    }
}

impl AskKind {
    pub fn permission(tool: impl Into<String>, subject: impl Into<String>, rememberable: bool) -> Self {
        Self::Permission { tool: tool.into(), subject: subject.into(), rememberable, disclosed: Disclosed::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub header: String,
    pub text: String,
    pub options: Vec<Choice>,
    pub multi: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Allow,
    AllowAlways,
    Deny,
    DenyWith(String),
    Answer(Vec<Vec<usize>>),
    Approve(PermissionMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PermissionMode {
    #[default]
    Auto,
    Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Reviewer {
    #[default]
    Model,
    Human,
    Nobody,
}

impl Reviewer {
    pub fn is_model(&self) -> bool {
        *self == Self::Model
    }
}

impl PermissionMode {
    pub const ALL: [PermissionMode; 2] = [Self::Auto, Self::Plan];

    pub const fn next(self) -> Self {
        match self {
            Self::Auto => Self::Plan,
            Self::Plan => Self::Auto,
        }
    }

    pub fn capped(self, ceiling: Option<PermissionMode>) -> PermissionMode {
        match ceiling {
            Some(Self::Plan) => Self::Plan,
            _ => self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_ceiling_keeps_the_agent_planning() {
        use PermissionMode::*;
        assert_eq!(Auto.capped(Some(Plan)), Plan);
        assert_eq!(Plan.capped(Some(Auto)), Plan);
        assert_eq!(Auto.capped(None), Auto);
    }

    #[test]
    fn auto_is_the_default_mode() {
        assert_eq!(PermissionMode::default(), PermissionMode::Auto);
    }
}
