use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Mutex, PoisonError};

use super::capture::Captured;

#[derive(Debug)]
pub struct Queue {
    sender: SyncSender<Captured>,
    receiver: Mutex<Option<Receiver<Captured>>>,
    dropped: AtomicU64,
}

impl Queue {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = sync_channel(capacity.max(1));
        Self { sender, receiver: Mutex::new(Some(receiver)), dropped: AtomicU64::new(0) }
    }

    pub fn offer(&self, captured: Captured) {
        if self.sender.try_send(captured).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn lost(&self, count: u64) {
        self.dropped.fetch_add(count, Ordering::Relaxed);
    }

    pub fn take_dropped(&self) -> u64 {
        self.dropped.swap(0, Ordering::Relaxed)
    }

    pub fn take_receiver(&self) -> Option<Receiver<Captured>> {
        self.receiver.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}
