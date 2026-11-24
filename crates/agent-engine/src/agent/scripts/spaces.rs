use serde_json::Value;

use super::{Script, Within};
use crate::agent::Agent;
use crate::tools::script_tool::Origin;

type Served = for<'env> fn(&'env Agent<'_>, Within<'_, 'env>, &Script<'_>, &str, &Value) -> Result<Value, String>;

const SPACES: &[(&str, Served)] = &[
    (super::user::SPACE, super::user::call),
    (super::plan::SPACE, super::plan::call),
    (super::watch::SPACE, super::watch::call),
    (super::repl::SPACE, super::repl::call),
    (super::skill::SPACE, super::skill::call),
    (super::agents::SPACE, super::agents::call),
    (super::background::SPACE, super::background::call),
    (super::peers::SPACE, super::peers::call),
];

pub(super) fn named(function: &str) -> Option<(Served, &str)> {
    let (space, rest) = function.split_once('.')?;
    SPACES.iter().find(|(known, _)| *known == space).map(|(_, served)| (*served, rest))
}

impl Agent<'_> {
    pub(super) fn loop_space<'env>(
        &'env self,
        within: Within<'_, 'env>,
        script: &Script<'_>,
        function: &str,
        arguments: &Value,
    ) -> Option<Result<Value, String>> {
        let (served, rest) = named(function)?;
        if script.origin != Origin::Owner {
            return Some(Err(format!("ctx.{function} is for the owner's tools")));
        }
        Some(served(self, within, script, rest, arguments))
    }
}

pub(super) fn reached(source: &str) -> bool {
    SPACES.iter().any(|(space, _)| source.contains(&format!("ctx.{space}.")))
}
