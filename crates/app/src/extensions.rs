use std::path::{Path, PathBuf};

use domain::PromptRequest;

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
}
