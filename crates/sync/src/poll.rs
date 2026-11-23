use connectors::ExternalEvent;
use domain::workflow::WorkflowEvent;

use crate::book::{Book, Entry};
use crate::desired::Desired;
use crate::error::Error;
use crate::runner::{PollReport, Synchronizer, observed};

impl Synchronizer {
    pub async fn poll_now(&self, connection: &str) -> Result<Option<PollReport>, Error> {
        let book = Book::load(&self.store)?;
        let Some(entry) = book.find(connection) else { return Ok(None) };
        let desired = self.active().await?;
        self.poll_connection(entry, &desired).await
    }

    pub(crate) async fn poll_connection(
        &self,
        entry: &Entry,
        desired: &[Desired],
    ) -> Result<Option<PollReport>, Error> {
        let id = entry.connection.id.as_str();
        let cursor = self.store.cursor(id)?;
        let Some(batch) = self.connectors.poll(&entry.connection, &entry.settings, cursor.as_deref()).await? else {
            return Ok(None);
        };
        let mut events = batch.events;
        events.sort_by_key(|event| event.at);
        let mut report = PollReport { fetched: events.len(), ..PollReport::default() };
        for event in &events {
            if self.store.was_seen(id, &event.id)? {
                report.duplicates += 1;
                continue;
            }
            self.route(event, desired, &mut report).await?;
            self.store.mark_seen(id, &event.id)?;
        }
        self.store.set_cursor(id, &batch.cursor)?;
        Ok(Some(report))
    }

    pub(crate) async fn route(
        &self,
        event: &ExternalEvent,
        desired: &[Desired],
        report: &mut PollReport,
    ) -> Result<usize, Error> {
        let delivered = self.sink.deliver(event).await?;
        let triggered = self.sink.trigger(event).await?;
        report.delivered += delivered;
        report.triggered += triggered;
        if delivered == 0 {
            for item in desired.iter().filter(|item| item.concerns(event)) {
                self.sink.record(&item.item, &[WorkflowEvent::ExternalObserved(observed(event))]).await?;
                report.observed += 1;
            }
        }
        Ok(delivered + triggered)
    }
}
