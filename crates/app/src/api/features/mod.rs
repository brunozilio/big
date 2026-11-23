pub mod code_map;
pub mod flags;
pub mod memory;
pub mod output_savings;
pub mod plugins;
pub mod schedules;
pub mod sessions;
pub mod shell;
pub mod trace;
pub mod workspace;

use serde::{Deserialize, Serialize};

use self::code_map::{CodeMapReply, CodeMapRequest};
use self::flags::{FlagsReply, FlagsRequest};
use self::memory::{MemoryReply, MemoryRequest};
use self::output_savings::OutputSavingsReply;
use self::plugins::{PluginsReply, PluginsRequest};
use self::schedules::{SchedulesReply, SchedulesRequest};
use self::sessions::{SessionsReply, SessionsRequest};
use self::shell::{ShellReply, ShellRequest};
use self::trace::{TraceReply, TraceRequest};
use self::workspace::{WorkspaceReply, WorkspaceRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureName {
    CodeMap,
    Flags,
    Memory,
    OutputSavings,
    Plugins,
    Schedules,
    Sessions,
    Shell,
    Trace,
    Workspace,
}

impl FeatureName {
    pub const ALL: [Self; 10] = [
        Self::CodeMap,
        Self::Flags,
        Self::Memory,
        Self::OutputSavings,
        Self::Plugins,
        Self::Schedules,
        Self::Sessions,
        Self::Shell,
        Self::Trace,
        Self::Workspace,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::CodeMap => "code_map",
            Self::Flags => "flags",
            Self::Memory => "memory",
            Self::OutputSavings => "output_savings",
            Self::Plugins => "plugins",
            Self::Schedules => "schedules",
            Self::Sessions => "sessions",
            Self::Shell => "shell",
            Self::Trace => "trace",
            Self::Workspace => "workspace",
        }
    }

    pub fn switch(self) -> Option<&'static str> {
        use domain::features as flags;
        match self {
            Self::CodeMap => Some(flags::CODE_MAP),
            Self::OutputSavings => Some(flags::OUTPUT_SAVINGS),
            Self::Plugins => Some(flags::PLUGINS),
            Self::Schedules => Some(flags::SCHEDULES),
            Self::Trace => Some(flags::TRACE),
            Self::Flags | Self::Memory | Self::Sessions | Self::Shell | Self::Workspace => None,
        }
    }
}

impl std::fmt::Display for FeatureName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "feature", content = "request", rename_all = "snake_case")]
pub enum FeatureRequest {
    CodeMap(CodeMapRequest),
    Flags(FlagsRequest),
    Memory(MemoryRequest),
    OutputSavings,
    Plugins(PluginsRequest),
    Schedules(SchedulesRequest),
    Sessions(SessionsRequest),
    Shell(ShellRequest),
    Trace(TraceRequest),
    Workspace(WorkspaceRequest),
}

impl FeatureRequest {
    pub fn feature(&self) -> FeatureName {
        match self {
            Self::CodeMap(_) => FeatureName::CodeMap,
            Self::Flags(_) => FeatureName::Flags,
            Self::Memory(_) => FeatureName::Memory,
            Self::OutputSavings => FeatureName::OutputSavings,
            Self::Plugins(_) => FeatureName::Plugins,
            Self::Schedules(_) => FeatureName::Schedules,
            Self::Sessions(_) => FeatureName::Sessions,
            Self::Shell(_) => FeatureName::Shell,
            Self::Trace(_) => FeatureName::Trace,
            Self::Workspace(_) => FeatureName::Workspace,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "feature", content = "reply", rename_all = "snake_case")]
pub enum FeatureReply {
    CodeMap(CodeMapReply),
    Flags(FlagsReply),
    Memory(MemoryReply),
    OutputSavings(OutputSavingsReply),
    Plugins(PluginsReply),
    Schedules(SchedulesReply),
    Sessions(SessionsReply),
    Shell(ShellReply),
    Trace(TraceReply),
    Workspace(WorkspaceReply),
}

impl FeatureReply {
    pub fn feature(&self) -> FeatureName {
        match self {
            Self::CodeMap(_) => FeatureName::CodeMap,
            Self::Flags(_) => FeatureName::Flags,
            Self::Memory(_) => FeatureName::Memory,
            Self::OutputSavings(_) => FeatureName::OutputSavings,
            Self::Plugins(_) => FeatureName::Plugins,
            Self::Schedules(_) => FeatureName::Schedules,
            Self::Sessions(_) => FeatureName::Sessions,
            Self::Shell(_) => FeatureName::Shell,
            Self::Trace(_) => FeatureName::Trace,
            Self::Workspace(_) => FeatureName::Workspace,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum FeatureError {
    #[error("{feature} is turned off{}", back_on(*.feature))]
    FeatureOff { feature: FeatureName },
    #[error("{message}")]
    Failed { feature: FeatureName, message: String },
}

fn back_on(feature: FeatureName) -> String {
    feature.switch().map(|flag| format!(" (/features {flag} on)")).unwrap_or_default()
}

impl FeatureError {
    pub fn feature(&self) -> FeatureName {
        match self {
            Self::FeatureOff { feature } | Self::Failed { feature, .. } => *feature,
        }
    }
}

#[cfg(test)]
mod tests;
