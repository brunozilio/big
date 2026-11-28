use crate::billing::Cost;
use crate::origin::Attribution;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Charged<'a> {
    pub owner: &'a str,
    pub agent: &'a str,
    pub attribution: &'a Attribution,
    pub cost: Cost,
}

pub trait SpendSink: Send + Sync + std::fmt::Debug {
    fn spent(&self, charged: &Charged<'_>);
}
