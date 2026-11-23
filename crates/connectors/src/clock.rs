use std::sync::{Mutex, PoisonError};

use jiff::{SignedDuration, Timestamp};

pub trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> Timestamp;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

#[derive(Debug)]
pub struct FixedClock(Mutex<Timestamp>);

impl FixedClock {
    pub fn at_second(second: i64) -> FixedClock {
        FixedClock(Mutex::new(Timestamp::from_second(second).unwrap_or(Timestamp::UNIX_EPOCH)))
    }

    pub fn advance(&self, seconds: i64) {
        let mut now = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *now = now.checked_add(SignedDuration::from_secs(seconds)).unwrap_or(*now);
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub fn after_seconds(now: Timestamp, seconds: i64) -> Timestamp {
    now.checked_add(SignedDuration::from_secs(seconds)).unwrap_or(now)
}
