#[derive(Debug, Clone)]
pub struct Models {
    pub model: domain::ModelOption,
    pub fallbacks: Vec<String>,
    pub small_model: Option<String>,
    pub escalation: Option<String>,
    pub roles: domain::model_roles::ModelRoles,
    pub tiers: std::sync::Arc<domain::model_roles::Tiers>,
    pub requires: domain::data_policy::Requires,
}

impl Models {
    pub fn new(model: domain::ModelOption) -> Self {
        Self {
            model,
            fallbacks: Vec::new(),
            small_model: None,
            escalation: None,
            roles: Default::default(),
            tiers: crate::tiers::current(),
            requires: Default::default(),
        }
    }

    pub fn resolved<'a>(&'a self, name: &'a str, parent: &'a str) -> &'a str {
        match self.tiers.find(name) {
            Some(_) => self.roles.for_tier(&self.tiers, name, parent, self.small_model.as_deref()),
            None => name,
        }
    }

    pub fn unmet(&self, model: &str) -> Option<String> {
        crate::data_gate::unmet(&self.requires, model, self.model.provider)
    }

    pub fn switch_to(&mut self, id: String) {
        self.model.id = id;
        self.model.capabilities = None;
        self.model.window = None;
    }
}

impl Default for Models {
    fn default() -> Self {
        Self::new(domain::ModelOption::new("unknown", "Unknown"))
    }
}
