use std::path::{Path, PathBuf};

use domain::{CustomAgent, Folder, PromptRequest, Skill};

#[derive(Debug, Clone, Default)]
pub struct Extensions;

impl Extensions {
    pub(crate) fn host(&self, _cwd: &Path, home: Option<&Path>) -> domain::Extensions {
        domain::Extensions { home: home.map(Path::to_path_buf), ..Default::default() }
    }

    pub(crate) fn brief(&self, mut request: PromptRequest, home: Option<PathBuf>) -> PromptRequest {
        let extensions = self.host(&request.session.cwd, home.as_deref());
        request.host.extensions = (!extensions.is_empty()).then_some(extensions);
        request
    }
}

pub(crate) fn skills(cwd: &Path, extensions: &Extensions, home: Option<&Path>) -> Vec<Skill> {
    let host = extensions.host(cwd, home);
    agent_engine::tools::skill::discover(Folder::at(cwd, &host), home)
        .into_iter()
        .map(|found| Skill { name: found.name, description: found.description })
        .collect()
}

pub(crate) fn agents(cwd: &Path, extensions: &Extensions, home: Option<&Path>) -> Vec<CustomAgent> {
    let host = extensions.host(cwd, home);
    agent_engine::tools::subagent::discover(Folder::at(cwd, &host), home)
        .into_iter()
        .map(|found| CustomAgent { name: found.name, description: found.description })
        .collect()
}

#[cfg(test)]
mod tests {
    use domain::ModelOption;

    use super::*;

    #[test]
    fn a_run_without_a_home_gets_nothing_added() {
        let request = PromptRequest::plain("/repo".into(), "hi", ModelOption::new("m", "M"));
        assert_eq!(Extensions.brief(request.clone(), None), request);
    }

    #[test]
    fn a_run_gets_the_home_its_owner_was_given() {
        let request = PromptRequest::plain("/repo".into(), "hi", ModelOption::new("m", "M"));
        let briefed = Extensions.brief(request, Some("/users/ana".into()));
        assert_eq!(briefed.host.extensions.and_then(|extensions| extensions.home), Some(PathBuf::from("/users/ana")));
    }

    #[test]
    fn listings_read_the_given_home_not_the_machine_s() {
        let dir = tempfile::tempdir().expect("dir");
        let home = dir.path().join("ana");
        std::fs::create_dir_all(home.join(brand::DIR).join("agents")).expect("agents");
        std::fs::write(
            home.join(brand::DIR).join("agents/helper.md"),
            "---\nname: helper\ndescription: Helps\n---\nHelp.",
        )
        .expect("agent");
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).expect("project");
        let named = |home: Option<&Path>| agents(&project, &Extensions, home).iter().any(|a| a.name == "helper");
        assert!(named(Some(&home)));
        assert!(!named(None));
    }
}
