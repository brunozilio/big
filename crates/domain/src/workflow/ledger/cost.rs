use std::collections::BTreeMap;

use serde::Serialize;

use super::view::Window;
use crate::billing::Cost;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpendLine {
    pub item: String,
    pub step: String,
    pub micros: u64,
    pub unpriced: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Spend {
    pub usd: f64,
    pub unpriced_calls: u64,
}

impl Spend {
    pub fn of<'a>(lines: impl Iterator<Item = &'a SpendLine>) -> Self {
        let (micros, unpriced) = lines.fold((0_u64, 0_u64), |(micros, unpriced), line| {
            (micros.saturating_add(line.micros), unpriced.saturating_add(line.unpriced))
        });
        Self { usd: Cost { known_micros: micros, ..Cost::default() }.known_usd(), unpriced_calls: unpriced }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepCost {
    pub step: String,
    #[serde(flatten)]
    pub spend: Spend,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemCost {
    pub id: String,
    pub title: String,
    #[serde(flatten)]
    pub spend: Spend,
    pub steps: Vec<StepCost>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostView {
    pub person: Option<String>,
    #[serde(flatten)]
    pub window: Window,
    #[serde(flatten)]
    pub total: Spend,
    pub conversations: Spend,
    pub items: Vec<ItemCost>,
}

pub fn cost_view(
    person: Option<String>,
    window: Window,
    lines: &[SpendLine],
    title: &dyn Fn(&str) -> String,
) -> CostView {
    let mut by_item: BTreeMap<&str, Vec<&SpendLine>> = BTreeMap::new();
    for line in lines.iter().filter(|line| !line.item.is_empty()) {
        by_item.entry(line.item.as_str()).or_default().push(line);
    }
    let mut items: Vec<ItemCost> = by_item
        .into_iter()
        .map(|(id, lines)| {
            let mut steps: BTreeMap<&str, Vec<&SpendLine>> = BTreeMap::new();
            for line in lines.iter().copied() {
                steps.entry(line.step.as_str()).or_default().push(line);
            }
            let steps = steps
                .into_iter()
                .map(|(step, lines)| StepCost { step: step.to_owned(), spend: Spend::of(lines.into_iter()) })
                .collect();
            ItemCost { id: id.to_owned(), title: title(id), spend: Spend::of(lines.into_iter()), steps }
        })
        .collect();
    items.sort_by(|left, right| right.spend.usd.total_cmp(&left.spend.usd).then_with(|| left.id.cmp(&right.id)));
    CostView {
        person,
        window,
        total: Spend::of(lines.iter()),
        conversations: Spend::of(lines.iter().filter(|line| line.item.is_empty())),
        items,
    }
}
