use std::sync::{Arc, OnceLock};

use domain::billing::Cost;
use domain::origin::Attribution;
use domain::spend::{Charged, SpendSink};

static SINK: OnceLock<Arc<dyn SpendSink>> = OnceLock::new();

pub fn install(sink: Arc<dyn SpendSink>) {
    let _ = SINK.set(sink);
}

pub(crate) fn charge(owner: &str, agent: &str, attribution: &Attribution, cost: Cost) {
    if attribution.item.is_some() || cost == Cost::default() {
        return;
    }
    if let Some(sink) = SINK.get() {
        sink.spent(&Charged { owner, agent, attribution, cost });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use domain::origin::Origin;

    use super::*;

    #[derive(Debug, Default)]
    struct Kept(Mutex<Vec<(String, Origin, u64)>>);

    impl SpendSink for Kept {
        fn spent(&self, charged: &Charged<'_>) {
            let row = (charged.owner.to_owned(), charged.attribution.origin, charged.cost.known_micros);
            self.0.lock().expect("kept").push(row);
        }
    }

    #[test]
    fn a_chat_or_chore_spend_reaches_the_sink_with_its_owner_and_origin_and_an_item_s_does_not() {
        let kept = Arc::new(Kept::default());
        install(Arc::clone(&kept) as Arc<dyn SpendSink>);
        let cost = Cost::priced(Some(0.25), domain::billing::Basis::Api);
        let title = Attribution { origin: Origin::Title, ..Attribution::default() };
        charge("u-spend-sink-test", "", &title, cost);
        charge("u-spend-sink-test", "dev", &Attribution::of_step("i1", "develop", Origin::Step), cost);
        charge("u-spend-sink-test", "", &Attribution::default(), Cost::default());
        let rows: Vec<(String, Origin, u64)> =
            kept.0.lock().expect("kept").iter().filter(|row| row.0 == "u-spend-sink-test").cloned().collect();
        assert_eq!(rows, [("u-spend-sink-test".to_owned(), Origin::Title, 250_000)]);
    }
}
