use crate::api::Unsolicited;

use crate::services::LocalServices;

pub type Finish = Box<dyn FnOnce(&domain::memory::scorecard::RunOutcome) -> Option<Unsolicited> + Send + Sync>;

pub struct Context<'a> {
    pub services: &'a LocalServices,
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").finish_non_exhaustive()
    }
}

#[cfg(test)]
pub mod testing {
    use std::sync::Arc;

    use agent_engine::fake::FakeEngine;

    use super::*;

    pub fn services(data: Option<&std::path::Path>) -> LocalServices {
        let root = data.map_or_else(crate::DataRoot::in_memory, crate::DataRoot::under);
        LocalServices::in_root(Arc::new(FakeEngine::new(Vec::new(), Vec::new())), root)
    }

    pub fn with<T>(services: &LocalServices, body: impl FnOnce(&Context<'_>) -> T) -> T {
        body(&Context { services })
    }
}
