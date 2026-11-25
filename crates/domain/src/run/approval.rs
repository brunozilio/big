use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AskId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAsk {
    pub id: AskId,
    pub kind: AskKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AskKind {
    Permission { tool: String, subject: String, rememberable: bool },
    Questions(Vec<Question>),
    Plan { plan: String },
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
