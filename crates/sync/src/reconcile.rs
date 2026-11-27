use connectors::ExternalEvent;
use connectors::contracts::{CODE, PIPELINES, TRACKER, code, pipelines, tracker};
use domain::workflow::WorkflowEvent;
use futures_util::stream::{self, StreamExt};
use serde_json::{Value, json};

use crate::book::{Book, Entry};
use crate::desired::{CLOSED, Desired, FINISHED, MERGED, PullRef, RunRef, WAITING, kinds};
use crate::error::Error;
use crate::policy::FieldPolicy;
use crate::runner::{ReconcileReport, Synchronizer, observed, reconciled};

struct Divergence {
    field: &'static str,
    policy: FieldPolicy,
    local: Value,
    remote: Value,
    event: ExternalEvent,
    reapply: Option<(&'static str, Value)>,
    adopt: Vec<WorkflowEvent>,
}

fn same(local: &str, remote: &str) -> bool {
    local.trim().to_lowercase() == remote.trim().to_lowercase()
}

fn text<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name).and_then(Value::as_str).filter(|text| !text.is_empty())
}

impl Synchronizer {
    pub async fn reconcile_now(&self) -> Result<ReconcileReport, Error> {
        let book = crate::stored::stored(&self.store, Book::load).await?;
        let desired = self.active().await?;
        Ok(self.reconcile_with(&book, &desired).await)
    }

    pub(crate) async fn reconcile_with(&self, book: &Book, desired: &[Desired]) -> ReconcileReport {
        let each: Vec<_> = desired.iter().map(|item| self.reconcile_one(book, desired, item)).collect();
        let parts: Vec<ReconcileReport> = stream::iter(each).buffered(self.config().concurrency).collect().await;
        let mut report = ReconcileReport::default();
        for part in parts {
            report.items += part.items;
            report.corrections += part.corrections;
            report.errors.extend(part.errors);
        }
        report
    }

    async fn reconcile_one(&self, book: &Book, all: &[Desired], item: &Desired) -> ReconcileReport {
        let mut part = ReconcileReport { items: 1, ..ReconcileReport::default() };
        self.reconcile_item(book, all, item, &mut part).await;
        part
    }

    async fn reconcile_item(&self, book: &Book, all: &[Desired], item: &Desired, report: &mut ReconcileReport) {
        if let Some(issue) = &item.issue
            && let Some(entry) = book.resolve_for(&item.owner, issue.connection.as_deref(), TRACKER)
        {
            let outcome = self.reconcile_issue(entry, item, &issue.issue).await;
            self.tally(outcome, entry, book, all, report).await;
        }
        for pull in &item.pulls {
            if let Some(entry) = book.resolve_for(&item.owner, pull.connection.as_deref(), CODE) {
                let outcome = self.reconcile_pull(entry, item, pull).await;
                self.tally(outcome, entry, book, all, report).await;
            }
        }
        if !item.waits_for(PIPELINES) {
            return;
        }
        for run in item.runs.iter().filter(|run| !run.finished()) {
            if let Some(entry) = book.resolve_for(&item.owner, run.connection.as_deref(), PIPELINES) {
                let outcome = self.reconcile_run(entry, item, run).await;
                self.tally(outcome, entry, book, all, report).await;
            }
        }
    }

    async fn tally(
        &self,
        outcome: Result<usize, Error>,
        entry: &Entry,
        book: &Book,
        all: &[Desired],
        report: &mut ReconcileReport,
    ) {
        match outcome {
            Ok(corrections) => report.corrections += corrections,
            Err(error) => {
                if error.is_auth() {
                    let _ = self.revoke(&entry.connection.id, &error.to_string(), book, all).await;
                }
                report.errors.push(format!("{}: {error}", entry.connection.id));
            }
        }
    }

    async fn read(&self, entry: &Entry, action: &str, args: Value) -> Result<Value, Error> {
        Ok(self.connectors.call(&entry.connection, &entry.settings, action, &args, "").await?)
    }

    async fn reconcile_issue(&self, entry: &Entry, item: &Desired, issue: &str) -> Result<usize, Error> {
        let remote = self.read(entry, tracker::ISSUE, json!({ "issue": issue })).await?;
        let mut corrections = 0;
        if let (Some(local), Some(now)) = (item.status.as_deref(), text(&remote, "status"))
            && !same(local, now)
        {
            let data = json!({ "from": local, "to": now, "author": null, "reconciled": true });
            let id = format!("reconcile:{}:{issue}:status:{local}->{now}", item.item);
            corrections += self
                .settle(
                    entry,
                    item,
                    Divergence {
                        field: "status",
                        policy: item.policy.status,
                        local: json!(local),
                        remote: json!(now),
                        event: Synchronizer::synthetic(entry, TRACKER, kinds::STATUS_CHANGED, issue, data, id),
                        reapply: Some((tracker::TRANSITION, json!({ "issue": issue, "status": local }))),
                        adopt: vec![WorkflowEvent::StatusSet(now.to_owned())],
                    },
                )
                .await?;
        }
        if let Some(flagged) = remote.pointer("/impediment/on").and_then(Value::as_bool)
            && flagged != item.impediment.is_some()
        {
            corrections += self.settle(entry, item, impediment(entry, item, issue, flagged)).await?;
        }
        Ok(corrections)
    }

    async fn reconcile_pull(&self, entry: &Entry, item: &Desired, pull: &PullRef) -> Result<usize, Error> {
        let Some(local) = pull.state.as_deref() else { return Ok(0) };
        let remote = self.read(entry, code::PULL_REQUEST, json!({ "pr": pull.reference })).await?;
        let Some(now) = text(&remote, "state") else { return Ok(0) };
        if same(local, now) {
            return Ok(0);
        }
        let kind = match now {
            MERGED => kinds::PR_MERGED,
            CLOSED => kinds::PR_CLOSED,
            _ => kinds::PR_UPDATED,
        };
        let subject = pull.subject();
        let data = json!({ "pr": pull.reference, "state": now, "head": remote["head"] });
        let id = format!("reconcile:{}:{subject}:{local}->{now}", item.item);
        let event = Synchronizer::synthetic(entry, CODE, kind, &subject, data, id);
        let adopt = vec![WorkflowEvent::ExternalObserved(observed(&event))];
        let divergence = Divergence {
            field: "pull_request",
            policy: item.policy.pull_request,
            local: json!(local),
            remote: json!(now),
            event,
            reapply: None,
            adopt,
        };
        self.settle(entry, item, divergence).await
    }

    async fn reconcile_run(&self, entry: &Entry, item: &Desired, run: &RunRef) -> Result<usize, Error> {
        let remote = self.read(entry, pipelines::RUN, json!({ "run": run.reference })).await?;
        let Some(now) = text(&remote, "state") else { return Ok(0) };
        let kind = match now {
            state if FINISHED.contains(&state) => kinds::RUN_FINISHED,
            WAITING => kinds::DEPLOYMENT_WAITING,
            _ => return Ok(0),
        };
        let local = run.state.clone().unwrap_or_default();
        if same(&local, now) {
            return Ok(0);
        }
        let subject = run.subject();
        let data = json!({ "run": run.reference, "state": now, "head": remote["head"] });
        let id = format!("reconcile:{}:{subject}:{now}:{}", item.item, remote["attempt"]);
        let event = Synchronizer::synthetic(entry, PIPELINES, kind, &subject, data, id);
        let adopt = vec![WorkflowEvent::ExternalObserved(observed(&event))];
        let divergence = Divergence {
            field: "pipeline",
            policy: item.policy.pipeline,
            local: json!(local),
            remote: json!(now),
            event,
            reapply: None,
            adopt,
        };
        self.settle(entry, item, divergence).await
    }

    async fn settle(&self, entry: &Entry, item: &Desired, divergence: Divergence) -> Result<usize, Error> {
        let action = match divergence.policy {
            FieldPolicy::Remote => {
                self.sink.record(&item.item, &divergence.adopt).await?;
                "adopted"
            }
            FieldPolicy::Local => self.keep_local(entry, item, &divergence).await?,
            FieldPolicy::Event => match self.announce(entry, &divergence.event).await? {
                None => return Ok(0),
                Some(0) => self.keep_local(entry, item, &divergence).await?,
                Some(_) => "event",
            },
        };
        let event = reconciled(divergence.field, divergence.local, divergence.remote, action);
        self.sink.record(&item.item, &[event]).await?;
        Ok(1)
    }

    async fn keep_local(&self, entry: &Entry, item: &Desired, divergence: &Divergence) -> Result<&'static str, Error> {
        let Some((action, args)) = &divergence.reapply else {
            self.sink.record(&item.item, &divergence.adopt).await?;
            return Ok("observed");
        };
        let key = format!("{}/reconcile/{}", item.item, divergence.field);
        self.connectors.call(&entry.connection, &entry.settings, action, args, &key).await?;
        Ok("reapplied")
    }

    async fn announce(&self, entry: &Entry, event: &ExternalEvent) -> Result<Option<usize>, Error> {
        let connection = entry.connection.id.as_str();
        if self.store.was_seen(connection, &event.id)? {
            return Ok(None);
        }
        let handled = self.sink.deliver(event).await? + self.sink.trigger(event).await?;
        if handled > 0 {
            self.store.mark_seen(connection, &event.id)?;
        }
        Ok(Some(handled))
    }
}

fn impediment(entry: &Entry, item: &Desired, issue: &str, flagged: bool) -> Divergence {
    let local = item.impediment.is_some();
    let reason = item.impediment.as_ref().and_then(|impediment| impediment.reason.clone());
    let reapply = match &reason {
        Some(reason) => json!({ "issue": issue, "on": local, "reason": reason }),
        None => json!({ "issue": issue, "on": local }),
    };
    let data = json!({ "from": local, "to": flagged, "reconciled": true });
    let id = format!("reconcile:{}:{issue}:impediment:{local}->{flagged}", item.item);
    Divergence {
        field: "impediment",
        policy: item.policy.impediment,
        local: json!(local),
        remote: json!(flagged),
        event: Synchronizer::synthetic(entry, TRACKER, kinds::IMPEDIMENT_CHANGED, issue, data, id),
        reapply: Some((tracker::SET_IMPEDIMENT, reapply)),
        adopt: vec![WorkflowEvent::ImpedimentSet { on: flagged, reason: None }],
    }
}
