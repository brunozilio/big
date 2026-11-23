use std::process::Command;

use super::Workspace;

#[derive(Debug, Clone, Default)]
pub struct Starter {
    launch: super::Launch,
}

impl Starter {
    pub fn launched(&self, command: Command) -> Command {
        self.launch.0.launch(command)
    }
}

impl Workspace {
    pub fn starter(&self) -> Starter {
        Starter { launch: self.services.launch.clone() }
    }

    pub fn launched(&self, command: Command) -> Command {
        self.services.launch.0.launch(command)
    }
}
