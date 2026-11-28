use std::collections::BTreeSet;

use crate::descriptor::ProviderDescriptor;

impl ProviderDescriptor {
    pub fn published(&self) -> BTreeSet<String> {
        let key = self.key.to_string();
        let mut ids = BTreeSet::from([format!("connector:{key}")]);
        ids.extend(self.actions.iter().map(|action| format!("action:{key}/{}", action.name)));
        ids
    }
}
