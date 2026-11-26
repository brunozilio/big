use std::collections::HashMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInfo {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub version: String,
}

#[derive(Debug)]
struct Linked {
    connection: u64,
    info: BrowserInfo,
    outbox: UnboundedSender<String>,
}

#[derive(Debug)]
struct Pending {
    connection: u64,
    reply: Sender<Result<Value, String>>,
}

#[derive(Debug, Default)]
struct State {
    next: u64,
    browsers: Vec<Linked>,
    pending: HashMap<u64, Pending>,
    chosen: HashMap<String, String>,
    pairing: HashMap<u64, Sender<(String, String)>>,
    captured: HashMap<String, Value>,
    build: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Browser(String),
    NoneConnected,
    Several(Vec<BrowserInfo>),
}

#[derive(Debug, Clone, Default)]
pub struct Hub(Arc<Mutex<State>>);

pub const DISCONNECTED: &str = "The browser extension disconnected before it answered. Check that Chrome is still open, then call tabs_context_mcp and try again.";

impl Hub {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn link(&self, info: BrowserInfo, outbox: UnboundedSender<String>) -> u64 {
        let mut state = self.state();
        state.next += 1;
        let connection = state.next;
        let device = info.device_id.clone();
        let stale: Vec<u64> = state
            .browsers
            .iter()
            .filter(|linked| linked.info.device_id == device)
            .map(|linked| linked.connection)
            .collect();
        state.browsers.retain(|linked| linked.info.device_id != device);
        state.browsers.push(Linked { connection, info, outbox });
        drop(state);
        for old in stale {
            self.fail_pending(old);
        }
        connection
    }

    pub fn expect(&self, build: &str) {
        self.state().build = Some(build.to_owned()).filter(|build| !build.is_empty());
    }

    pub fn outdated(&self, build: &str) -> Option<String> {
        self.state().build.clone().filter(|expected| expected != build)
    }

    pub fn unlink(&self, connection: u64) {
        self.state().browsers.retain(|linked| linked.connection != connection);
        self.fail_pending(connection);
    }

    fn fail_pending(&self, connection: u64) {
        let mut state = self.state();
        let gone: Vec<u64> =
            state.pending.iter().filter(|(_, pending)| pending.connection == connection).map(|(id, _)| *id).collect();
        for id in gone {
            if let Some(pending) = state.pending.remove(&id) {
                let _ = pending.reply.send(Err(DISCONNECTED.to_owned()));
            }
        }
    }

    pub fn browsers(&self) -> Vec<BrowserInfo> {
        self.state().browsers.iter().map(|linked| linked.info.clone()).collect()
    }

    pub fn rename(&self, device: &str, name: &str) {
        let mut state = self.state();
        if let Some(linked) = state.browsers.iter_mut().find(|linked| linked.info.device_id == device) {
            name.clone_into(&mut linked.info.name);
        }
    }

    pub fn choose(&self, session: &str, device: &str) -> bool {
        let mut state = self.state();
        let known = state.browsers.iter().any(|linked| linked.info.device_id == device);
        if known {
            state.chosen.insert(session.to_owned(), device.to_owned());
        }
        known
    }

    pub fn chosen(&self, session: &str) -> Option<String> {
        let state = self.state();
        let device = state.chosen.get(session)?;
        state.browsers.iter().any(|linked| &linked.info.device_id == device).then(|| device.clone())
    }

    pub fn resolve(&self, session: &str) -> Resolved {
        if let Some(device) = self.chosen(session) {
            return Resolved::Browser(device);
        }
        let browsers = self.browsers();
        match browsers.as_slice() {
            [] => Resolved::NoneConnected,
            [only] => Resolved::Browser(only.device_id.clone()),
            _ => Resolved::Several(browsers),
        }
    }

    pub fn answer(&self, id: u64, result: Result<Value, String>) {
        if let Some(pending) = self.state().pending.remove(&id) {
            let _ = pending.reply.send(result);
        }
    }

    pub fn call(&self, device: &str, message: Value, wait: Duration) -> Result<Value, String> {
        let (id, receiver) = self.send(device, message)?;
        let answer = receiver.recv_timeout(wait);
        self.state().pending.remove(&id);
        match answer {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(format!(
                "The browser did not answer within {} seconds. The page may be stuck or showing a dialog; take a screenshot to check.",
                wait.as_secs()
            )),
            Err(RecvTimeoutError::Disconnected) => Err(DISCONNECTED.to_owned()),
        }
    }

    fn send(&self, device: &str, mut message: Value) -> Result<(u64, Receiver<Result<Value, String>>), String> {
        let mut state = self.state();
        let Some(linked) = state.browsers.iter().find(|linked| linked.info.device_id == device) else {
            return Err(DISCONNECTED.to_owned());
        };
        let (connection, outbox) = (linked.connection, linked.outbox.clone());
        state.next += 1;
        let id = state.next;
        let (reply, receiver) = channel();
        state.pending.insert(id, Pending { connection, reply });
        drop(state);
        message["id"] = json!(id);
        if outbox.send(message.to_string()).is_err() {
            self.state().pending.remove(&id);
            return Err(DISCONNECTED.to_owned());
        }
        Ok((id, receiver))
    }

    pub fn pair(&self, session: &str, wait: Duration) -> Result<BrowserInfo, String> {
        let (sender, receiver) = channel();
        let (id, outboxes) = {
            let mut state = self.state();
            state.next += 1;
            let id = state.next;
            state.pairing.insert(id, sender);
            (id, state.browsers.iter().map(|linked| linked.outbox.clone()).collect::<Vec<_>>())
        };
        if outboxes.is_empty() {
            self.state().pairing.remove(&id);
            return Err(no_browser());
        }
        let request = json!({ "type": "pair", "id": id }).to_string();
        for outbox in &outboxes {
            let _ = outbox.send(request.clone());
        }
        let answer = receiver.recv_timeout(wait);
        let ended = json!({ "type": "pair_ended", "id": id }).to_string();
        self.state().pairing.remove(&id);
        for outbox in &outboxes {
            let _ = outbox.send(ended.clone());
        }
        let (device, name) = answer.map_err(|_| {
            format!("Nobody clicked 'Connect' in a browser within {} seconds. Ask the user which browser to use, or try switch_browser again.", wait.as_secs())
        })?;
        if !name.trim().is_empty() {
            self.rename(&device, name.trim());
        }
        self.choose(session, &device);
        self.browsers().into_iter().find(|browser| browser.device_id == device).ok_or_else(|| DISCONNECTED.to_owned())
    }

    pub fn watch(&self, device: &str, flow: &str, url: &str, capture: &[String]) -> bool {
        let state = self.state();
        let Some(linked) = state.browsers.iter().find(|linked| linked.info.device_id == device) else { return false };
        let message = json!({ "type": "watch", "flow": flow, "url": url, "capture": capture });
        linked.outbox.send(message.to_string()).is_ok()
    }

    pub fn captured(&self, flow: &str, found: Value) {
        self.state().captured.insert(flow.to_owned(), found);
    }

    pub fn take_captured(&self, flow: &str) -> Option<Value> {
        self.state().captured.remove(flow)
    }

    pub fn paired(&self, id: u64, device: &str, name: &str) {
        if let Some(sender) = self.state().pairing.remove(&id) {
            let _ = sender.send((device.to_owned(), name.to_owned()));
        }
    }
}

pub fn no_browser() -> String {
    brand::text(
        "Browser extension is not connected. Ask the user to open Chrome with the {{NAME}} browser extension installed and signed in to this app (the extension's icon shows a green dot when it is connected), then try again.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(device: &str) -> BrowserInfo {
        BrowserInfo {
            device_id: device.into(),
            name: format!("Chrome {device}"),
            platform: "mac".into(),
            version: "1".into(),
        }
    }

    #[test]
    fn one_browser_is_used_without_asking_and_several_need_a_choice() {
        let hub = Hub::new();
        assert_eq!(hub.resolve("s"), Resolved::NoneConnected);
        let (a, _ra) = tokio::sync::mpsc::unbounded_channel();
        let first = hub.link(info("a"), a);
        assert_eq!(hub.resolve("s"), Resolved::Browser("a".into()));
        let (b, _rb) = tokio::sync::mpsc::unbounded_channel();
        hub.link(info("b"), b);
        assert!(matches!(hub.resolve("s"), Resolved::Several(list) if list.len() == 2));
        assert!(hub.choose("s", "b"));
        assert_eq!(hub.resolve("s"), Resolved::Browser("b".into()));
        assert!(matches!(hub.resolve("other"), Resolved::Several(_)));
        hub.unlink(first);
        assert_eq!(hub.resolve("other"), Resolved::Browser("b".into()));
        assert!(!hub.choose("s", "gone"));
    }

    #[test]
    fn a_call_gets_the_answer_with_its_id_and_fails_when_the_browser_leaves() {
        let hub = Hub::new();
        let (outbox, mut inbox) = tokio::sync::mpsc::unbounded_channel();
        let connection = hub.link(info("a"), outbox);
        let answering = hub.clone();
        let worker = std::thread::spawn(move || {
            let sent: Value = serde_json::from_str(&inbox.blocking_recv().unwrap_or_default()).unwrap_or_default();
            let id = sent["id"].as_u64().unwrap_or_default();
            answering.answer(id, Ok(json!({ "echo": sent["tool"] })));
            let second: Value = serde_json::from_str(&inbox.blocking_recv().unwrap_or_default()).unwrap_or_default();
            assert!(second["id"].is_u64());
            answering.unlink(connection);
        });
        let answer = hub.call("a", json!({ "type": "call", "tool": "find" }), Duration::from_secs(5));
        assert_eq!(answer, Ok(json!({ "echo": "find" })));
        let gone = hub.call("a", json!({ "type": "call", "tool": "find" }), Duration::from_secs(5));
        assert_eq!(gone, Err(DISCONNECTED.to_owned()));
        worker.join().unwrap_or_default();
    }

    #[test]
    fn a_reconnecting_browser_replaces_its_old_link() {
        let hub = Hub::new();
        let (a, _ra) = tokio::sync::mpsc::unbounded_channel();
        hub.link(info("a"), a);
        let (again, _rb) = tokio::sync::mpsc::unbounded_channel();
        hub.link(info("a"), again);
        assert_eq!(hub.browsers().len(), 1);
    }

    #[test]
    fn a_watch_reaches_the_browser_and_its_capture_is_taken_once() {
        let hub = Hub::new();
        assert!(!hub.watch("a", "f1", "https://x.test/grant", &["*/grant*".to_owned()]));
        let (outbox, mut inbox) = tokio::sync::mpsc::unbounded_channel();
        hub.link(info("a"), outbox);
        assert!(hub.watch("a", "f1", "https://x.test/grant", &["*/grant*".to_owned()]));
        let sent: Value = serde_json::from_str(&inbox.try_recv().unwrap_or_default()).unwrap_or_default();
        assert_eq!(
            sent,
            json!({ "type": "watch", "flow": "f1", "url": "https://x.test/grant", "capture": ["*/grant*"] })
        );
        hub.captured("f1", json!({ "status": 200 }));
        assert_eq!(hub.take_captured("f1"), Some(json!({ "status": 200 })));
        assert_eq!(hub.take_captured("f1"), None);
    }

    #[test]
    fn pairing_picks_the_browser_that_connects_and_renames_it() {
        let hub = Hub::new();
        let (outbox, mut inbox) = tokio::sync::mpsc::unbounded_channel();
        hub.link(info("a"), outbox);
        let answering = hub.clone();
        let worker = std::thread::spawn(move || {
            let sent: Value = serde_json::from_str(&inbox.blocking_recv().unwrap_or_default()).unwrap_or_default();
            answering.paired(sent["id"].as_u64().unwrap_or_default(), "a", "Work laptop");
        });
        let paired = hub.pair("s", Duration::from_secs(5));
        worker.join().unwrap_or_default();
        assert_eq!(paired.map(|browser| browser.name), Ok("Work laptop".to_owned()));
        assert_eq!(hub.chosen("s"), Some("a".to_owned()));
    }
}
