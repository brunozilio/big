use std::process::Command;

use super::Workspace;

#[derive(Debug, Clone, Default)]
pub struct Starter {
    launch: super::Launch,
    environment: Vec<(String, String)>,
    owner: Option<String>,
}

impl Starter {
    pub fn launched(&self, command: Command) -> std::io::Result<Command> {
        confined(self.launch.0.launch(with_environment(command, &self.environment)), self.owner.as_deref())
    }
}

fn confined(mut command: Command, owner: Option<&str>) -> std::io::Result<Command> {
    platform::confine::as_owner(&mut command, owner)?;
    Ok(command)
}

fn with_environment(mut command: Command, environment: &[(String, String)]) -> Command {
    command.envs(environment.iter().map(|(name, value)| (name, value)));
    command
}

impl Workspace {
    pub fn starter(&self) -> Starter {
        Starter {
            launch: self.services.launch.clone(),
            environment: self.environment.clone(),
            owner: self.owner.clone(),
        }
    }

    pub fn launched(&self, command: Command) -> std::io::Result<Command> {
        confined(self.services.launch.0.launch(with_environment(command, &self.environment)), self.owner.as_deref())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn printed(mut command: Command) -> String {
        command.output().map(|done| String::from_utf8_lossy(&done.stdout).trim().to_owned()).unwrap_or_default()
    }

    fn with_cloud() -> Workspace {
        let mut workspace = Workspace::new(std::env::temp_dir());
        workspace.environment = vec![("CLOUD_PROFILE_PROBE".into(), "run-1".into())];
        workspace
    }

    #[test]
    fn a_shell_the_workspace_starts_sees_the_variables_of_the_request() {
        let workspace = with_cloud();
        let shell = workspace.launched(platform::shell("printf %s \"$CLOUD_PROFILE_PROBE\"")).expect("launched");
        assert_eq!(printed(shell), "run-1");
    }

    #[test]
    fn scripts_and_forked_workspaces_carry_the_same_variables() {
        let workspace = with_cloud();
        let script = workspace.starter().launched(platform::shell("printf %s \"$CLOUD_PROFILE_PROBE\"")).expect("launched");
        assert_eq!(printed(script), "run-1");
        let child = workspace.fork(Arc::default());
        assert_eq!(printed(child.launched(platform::shell("printf %s \"$CLOUD_PROFILE_PROBE\"")).expect("launched")), "run-1");
    }

    #[test]
    fn a_request_hands_its_environment_to_the_workspace() {
        let mut request = domain::PromptRequest::plain(
            std::env::temp_dir(),
            String::new(),
            domain::ModelOption {
                id: "m".into(),
                label: "m".into(),
                provider: Default::default(),
                window: None,
                capabilities: None,
            },
        );
        request.host.environment = vec![("AWS_PROFILE".into(), "agent".into())];
        let workspace = Workspace::for_request(&request);
        assert_eq!(workspace.environment, vec![("AWS_PROFILE".to_owned(), "agent".to_owned())]);
        assert!(!serde_json::to_string(&request).expect("json").contains("AWS_PROFILE"));
    }
}
