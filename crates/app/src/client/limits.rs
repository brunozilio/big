use domain::billing::AccountLimits;

use crate::Client;

impl Client {
    pub fn plan_limits(&self, language: domain::Language) -> Vec<AccountLimits> {
        self.services.providers.plan_limits(language)
    }
}
