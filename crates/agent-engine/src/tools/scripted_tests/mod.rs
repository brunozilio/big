mod files;
mod platform;
mod session;
mod shell;
mod web;

use super::script_tool::{ScriptTool, builtin};

pub(crate) fn shipped(name: &str) -> ScriptTool {
    builtin().into_iter().find(|tool| tool.spec.name == name).unwrap_or_else(|| panic!("{name} is a built-in script"))
}
