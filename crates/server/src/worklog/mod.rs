mod access;

use std::collections::BTreeSet;
use std::sync::Arc;

use domain::redact::{Known, Redactor};
use domain::workflow::ledger::{Ask, CostView, DAY_MS, ItemView, Ledger, SpendLine, cost_view, item_view, title_of};
use domain::workflow::{ItemId, WorkItem};
use store::{SpendOf, Store};

use access::{Reader, Whose, may_read, whose};

use crate::roles::Roles;

const MOST_DAYS: i64 = 31;

fn shown(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub fn most_days(configured: &str) -> i64 {
    configured.trim().parse::<i64>().ok().filter(|days| *days > 0).unwrap_or(MOST_DAYS)
}

#[derive(Debug)]
pub struct StoredWorklog {
    store: Arc<Store>,
    shared: bool,
    roles: Roles,
    most_ms: i64,
    redactor: Arc<Redactor>,
}

impl StoredWorklog {
    pub fn new(store: Arc<Store>, shared: bool, most_days: i64, redactor: Arc<Redactor>) -> Self {
        let most_ms = most_days.max(1).saturating_mul(DAY_MS);
        Self { store, shared, roles: Roles::configured(), most_ms, redactor }
    }

    pub fn configured(store: Arc<Store>, redactor: Arc<Redactor>) -> Self {
        Self::new(store, crate::scope::shared_mode(), most_days(brand::worklog::MAX_DAYS), redactor)
    }

    pub fn with_roles(self, roles: Roles) -> Self {
        Self { roles, ..self }
    }

    fn reader(&self) -> Reader<'_> {
        Reader { store: &self.store, roles: &self.roles, shared: self.shared }
    }

    fn bounded(&self, ask: &Ask) -> Ask {
        let floor = ask.until_ms.saturating_sub(self.most_ms);
        Ask { since_ms: ask.since_ms.clamp(floor, ask.until_ms), ..ask.clone() }
    }

    fn clean(&self, text: &str) -> String {
        self.redactor.redact(text, &Known::default()).text
    }

    fn lines(&self, ask: &Ask, of: SpendOf<'_>) -> Result<Vec<SpendLine>, String> {
        let rows = self.store.spend_between(ask.since_ms, ask.until_ms, of).map_err(shown)?;
        Ok(rows
            .into_iter()
            .map(|row| SpendLine { item: row.item, step: row.step, micros: row.micros, unpriced: row.unpriced })
            .collect())
    }

    fn view(&self, item: &WorkItem, owner: &str, ask: &Ask, lines: &[SpendLine]) -> Result<ItemView, String> {
        let events = self.store.timed_events(&item.id, 0).map_err(shown)?;
        Ok(item_view(item, owner, &events, ask.window(), lines).redacted(&|text| self.clean(text)))
    }

    fn chosen(&self, ask: &Ask, whose: &Whose) -> Result<Vec<(WorkItem, String)>, String> {
        let touched = self.store.touched_items(ask.since_ms, ask.until_ms).map_err(shown)?;
        let mut seen: BTreeSet<ItemId> = BTreeSet::new();
        let mut kept = Vec::new();
        for (id, _) in touched {
            let Some(item) = self.store.item(&id).map_err(shown)? else { continue };
            seen.insert(id);
            kept.push(item);
        }
        let active = self.store.active_items().map_err(shown)?;
        kept.extend(active.into_iter().filter(|item| !seen.contains(&item.id)));
        let mut chosen = Vec::new();
        for item in kept {
            if chosen.len() == ask.limit {
                break;
            }
            let owner = orchestrator::owners::of(&self.store, &item);
            if whose.holds(&owner) {
                chosen.push((item, owner));
            }
        }
        Ok(chosen)
    }
}

fn owner_of(whose: &Whose) -> Option<&str> {
    match whose {
        Whose::Everyone => None,
        Whose::Person(person) => Some(person),
    }
}

impl Ledger for StoredWorklog {
    fn items(&self, asker: &str, ask: &Ask) -> Result<Vec<ItemView>, String> {
        let ask = self.bounded(ask);
        let whose = whose(self.reader(), asker, ask.person.as_deref())?;
        let lines = self.lines(&ask, SpendOf { item: None, owner: owner_of(&whose) })?;
        self.chosen(&ask, &whose)?.iter().map(|(item, owner)| self.view(item, owner, &ask, &lines)).collect()
    }

    fn item(&self, asker: &str, id: &str, ask: &Ask) -> Result<ItemView, String> {
        let ask = self.bounded(ask);
        let hidden = || format!("there is no work item {id} you can read");
        let id = ItemId::from(id);
        let item = self.store.item(&id).map_err(shown)?.ok_or_else(hidden)?;
        let owner = orchestrator::owners::of(&self.store, &item);
        if !may_read(self.reader(), asker, &owner)? {
            return Err(hidden());
        }
        let lines = self.lines(&ask, SpendOf { item: Some(&id), owner: None })?;
        self.view(&item, &owner, &ask, &lines)
    }

    fn cost(&self, asker: &str, ask: &Ask) -> Result<CostView, String> {
        let ask = self.bounded(ask);
        let whose = whose(self.reader(), asker, ask.person.as_deref())?;
        let lines = self.lines(&ask, SpendOf { item: None, owner: owner_of(&whose) })?;
        let ids: BTreeSet<&str> = lines.iter().map(|line| line.item.as_str()).filter(|id| !id.is_empty()).collect();
        let mut titles = std::collections::BTreeMap::new();
        for id in ids {
            let found = self.store.item(&ItemId::from(id)).map_err(shown)?;
            titles.insert(id.to_owned(), self.clean(&found.map_or_else(|| id.to_owned(), |item| title_of(&item))));
        }
        let title = |id: &str| titles.get(id).cloned().unwrap_or_else(|| id.to_owned());
        Ok(cost_view(whose.named(), ask.window(), &lines, &title))
    }
}

#[cfg(test)]
mod tests;
