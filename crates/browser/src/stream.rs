use futures_util::Stream;
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::task::JoinHandle;

use crate::hub::{BrowserInfo, Hub};
use crate::socket::{KEEP_ALIVE, parse_hello, receive};

pub fn hello(sent: &Value) -> Option<(BrowserInfo, String)> {
    let mut hello = sent.clone();
    hello["type"] = json!("hello");
    let build = hello["build"].as_str().unwrap_or_default().to_owned();
    parse_hello(&hello).map(|info| (info, build))
}

struct Open {
    hub: Hub,
    connection: u64,
    inbox: UnboundedReceiver<String>,
    pinger: JoinHandle<()>,
}

impl Drop for Open {
    fn drop(&mut self) {
        self.pinger.abort();
        self.hub.unlink(self.connection);
        tracing::info!(connection = self.connection, "a browser stream closed");
    }
}

pub fn open(
    hub: &Hub,
    info: BrowserInfo,
    build: &str,
    owner: Option<String>,
) -> impl Stream<Item = String> + Send + 'static {
    let (outbox, inbox) = unbounded_channel();
    if let Some(expected) = hub.outdated(build) {
        let _ = outbox.send(json!({ "type": "reload", "build": expected }).to_string());
    }
    let weak = outbox.downgrade();
    tracing::info!(device = %info.device_id, name = %info.name, "a browser connected by stream");
    let connection = hub.link_owned(info, owner, outbox);
    let pinger = tokio::spawn(async move {
        let mut every = tokio::time::interval(KEEP_ALIVE);
        let ping = json!({ "type": "ping" }).to_string();
        loop {
            every.tick().await;
            let Some(outbox) = weak.upgrade() else { break };
            if outbox.send(ping.clone()).is_err() {
                break;
            }
        }
    });
    let open = Open { hub: hub.clone(), connection, inbox, pinger };
    futures_util::stream::unfold(open, |mut open| async move {
        let next = open.inbox.recv().await?;
        Some((next, open))
    })
}

pub fn reply(hub: &Hub, device: &str, text: &str) {
    receive(hub, device, text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn a_stream_carries_calls_and_ends_when_the_browser_connects_again() {
        let hub = Hub::new();
        hub.expect("new");
        let (info, build) = hello(&json!({ "deviceId": "a", "build": "old" })).unwrap_or_else(|| unreachable!());
        let mut first = Box::pin(open(&hub, info.clone(), &build, Some("ana".into())));
        let reload: Value = serde_json::from_str(&first.next().await.unwrap_or_default()).unwrap_or_default();
        assert_eq!(reload, json!({ "type": "reload", "build": "new" }));
        assert_eq!(hub.owned_by("ana").len(), 1);
        assert!(hub.watch("a", "f", "https://x.test", &["*".to_owned()]));
        let watched: Value = serde_json::from_str(&first.next().await.unwrap_or_default()).unwrap_or_default();
        assert_eq!(watched["type"], "watch");
        let second = Box::pin(open(&hub, info, "new", Some("ana".into())));
        while let Some(left) = first.next().await {
            assert!(left.contains("ping"));
        }
        drop(first);
        assert_eq!(hub.owned_by("ana").len(), 1);
        drop(second);
        assert!(hub.owned_by("ana").is_empty());
    }

    #[test]
    fn a_hello_without_a_device_is_refused() {
        assert_eq!(hello(&json!({ "build": "x" })), None);
    }
}
