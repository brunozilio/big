pub mod code_map;
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
    pub const ALL: [Self; 9] = [
        Self::CodeMap,
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

    pub fn on(self) -> bool {
        use brand::features as flags;
        let flag = match self {
            Self::CodeMap => flags::CODE_MAP,
            Self::OutputSavings => flags::OUTPUT_SAVINGS,
            Self::Plugins => flags::PLUGINS,
            Self::Schedules => flags::SCHEDULES,
            Self::Trace => flags::TRACE,
            Self::Memory | Self::Sessions | Self::Shell | Self::Workspace => return true,
        };
        domain::features::on(flag)
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
    #[error("{feature} is turned off")]
    FeatureOff { feature: FeatureName },
    #[error("{message}")]
    Failed { feature: FeatureName, message: String },
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
