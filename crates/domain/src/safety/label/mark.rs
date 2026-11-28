use serde::Serialize;

use super::{Levels, Read, Trust};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Mark {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip)]
    pub rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub external: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unlabelled: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sink<'a> {
    pub ceiling: Option<&'a str>,
    pub risky: bool,
}

fn kept(list: &mut Vec<String>, source: &str) {
    if !list.iter().any(|known| known == source) {
        list.push(source.to_owned());
    }
}

impl Mark {
    pub fn of<'a>(levels: &Levels, reads: impl IntoIterator<Item = &'a Read>) -> Mark {
        let mut mark = Mark::default();
        for read in reads {
            mark.add(levels, read);
        }
        mark
    }

    pub fn add(&mut self, levels: &Levels, read: &Read) {
        if read.trust == Trust::External {
            kept(&mut self.external, &read.source);
        }
        if read.label.is_none() {
            kept(&mut self.unlabelled, &read.source);
        }
        let rank = levels.ranked(read.label.as_deref());
        let unranked_first = self.rank.is_none() && self.label.is_none() && read.label.is_some();
        if rank > self.rank || unranked_first {
            self.rank = rank.or(self.rank);
            self.label = read.label.clone().or_else(|| levels.most_restricted().map(str::to_owned));
            self.from = Some(read.source.clone());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.label.is_none() && self.external.is_empty()
    }

    pub fn crossing(&self, levels: &Levels, sink: &Sink<'_>) -> Option<String> {
        if let Some(ceiling) = sink.ceiling
            && let (Some(rank), Some(top)) = (self.rank, levels.ceiling(ceiling))
            && rank > top
        {
            let label = self.label.as_deref().unwrap_or_default();
            let from = self.from.as_deref().unwrap_or_default();
            return Some(format!("would carry {label} data read from {from} past its {ceiling} ceiling"));
        }
        if sink.risky && !self.external.is_empty() {
            return Some(format!("acts after reading external content from {}", self.external.join(", ")));
        }
        None
    }
}
