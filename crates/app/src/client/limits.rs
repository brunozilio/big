use domain::billing::AccountLimits;

use crate::Client;

impl Client {
    pub fn plan_limits(&self) -> Vec<AccountLimits> {
        self.services.providers.plan_limits()
    }
}
