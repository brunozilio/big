mod access;

use std::collections::BTreeSet;
use std::sync::Arc;

use domain::workflow::ledger::{Ask, CostView, ItemView, Ledger, SpendLine, cost_view, item_view, title_of};
use domain::workflow::{ItemId, WorkItem};
use store::Store;

use access::{Whose, may_read, whose};

#[derive(Debug)]
pub struct StoredWorklog {
    store: Arc<Store>,
    shared: bool,
}

impl StoredWorklog {
    pub fn new(store: Arc<Store>, shared: bool) -> Self {
        Self { store, shared }
    }

    pub fn configured(store: Arc<Store>) -> Self {
        Self::new(store, crate::scope::shared_mode())
    }

    fn lines(&self, ask: &Ask, item: Option<&ItemId>, whose: &Whose) -> Result<Vec<SpendLine>, String> {
        let rows = self.store.spend_between(ask.since_ms, ask.until_ms, item).map_err(|error| error.to_string())?;
        Ok(rows
            .into_iter()
            .filter(|row| whose.holds(&row.owner))
            .map(|row| SpendLine { item: row.item, step: row.step, micros: row.micros, unpriced: row.unpriced })
            .collect())
    }

    fn view(&self, item: &WorkItem, owner: &str, ask: &Ask, lines: &[SpendLine]) -> Result<ItemView, String> {
        let events = self.store.timed_events(&item.id, 0).map_err(|error| error.to_string())?;
        Ok(item_view(item, owner, &events, ask.window(), lines))
    }

    fn candidates(&self, ask: &Ask) -> Result<Vec<WorkItem>, String> {
        let touched = self.store.touched_items(ask.since_ms, ask.until_ms).map_err(|error| error.to_string())?;
        let mut seen: BTreeSet<ItemId> = BTreeSet::new();
        let mut found = Vec::new();
        for id in touched {
            if let Ok(Some(item)) = self.store.item(&id) {
                seen.insert(id);
                found.push(item);
            }
        }
        let active = self.store.active_items().map_err(|error| error.to_string())?;
        found.extend(active.into_iter().filter(|item| !seen.contains(&item.id)));
        Ok(found)
    }
}

impl Ledger for StoredWorklog {
    fn items(&self, asker: &str, ask: &Ask) -> Result<Vec<ItemView>, String> {
        let whose = whose(&self.store, self.shared, asker, ask.person.as_deref())?;
        let lines = self.lines(ask, None, &whose)?;
        let mut views = Vec::new();
        for item in self.candidates(ask)? {
            let owner = orchestrator::owners::of(&self.store, &item);
            if whose.holds(&owner) {
                views.push(self.view(&item, &owner, ask, &lines)?);
            }
        }
        views.sort_by(|left, right| right.updated_ms.cmp(&left.updated_ms).then_with(|| left.id.cmp(&right.id)));
        views.truncate(ask.limit);
        Ok(views)
    }

    fn item(&self, asker: &str, id: &str, ask: &Ask) -> Result<ItemView, String> {
        let hidden = || format!("there is no work item {id} you can read");
        let id = ItemId::from(id);
        let item = self.store.item(&id).map_err(|error| error.to_string())?.ok_or_else(hidden)?;
        let owner = orchestrator::owners::of(&self.store, &item);
        if !may_read(&self.store, self.shared, asker, &owner) {
            return Err(hidden());
        }
        let lines = self.lines(ask, Some(&id), &Whose::Everyone)?;
        self.view(&item, &owner, ask, &lines)
    }

    fn cost(&self, asker: &str, ask: &Ask) -> Result<CostView, String> {
        let whose = whose(&self.store, self.shared, asker, ask.person.as_deref())?;
        let lines = self.lines(ask, None, &whose)?;
        let title = |id: &str| match self.store.item(&ItemId::from(id)) {
            Ok(Some(item)) => title_of(&item),
            _ => id.to_owned(),
        };
        Ok(cost_view(whose.named(), ask.window(), &lines, &title))
    }
}

#[cfg(test)]
mod tests;
