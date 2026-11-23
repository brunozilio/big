use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use platform::sync::Locked;

use domain::schedule::{MOST, Scheduled};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde_json::Value;

use crate::model::ToolSpec;
use crate::tools::{Outcome, Tool, Workspace, field, required};

fn lock() -> std::sync::MutexGuard<'static, HashMap<u64, Vec<Scheduled>>> {
    static ALL: OnceLock<Mutex<HashMap<u64, Vec<Scheduled>>>> = OnceLock::new();
    ALL.get_or_init(Mutex::default).locked()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScheduleError {
    #[error("At most {MOST} wakeups can be scheduled.")]
    Full,
}

impl From<ScheduleError> for String {
    fn from(error: ScheduleError) -> Self {
        error.to_string()
    }
}

pub fn add(client: u64, prompt: &str, at: i64) -> Result<Scheduled, ScheduleError> {
    let mut all = lock();
    let mine = all.entry(client).or_default();
    if mine.len() >= MOST {
        return Err(ScheduleError::Full);
    }
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = format!("wakeup_{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let scheduled = Scheduled { id, prompt: prompt.to_owned(), next: at };
    mine.push(scheduled.clone());
    Ok(scheduled)
}

pub fn list(client: u64) -> Vec<Scheduled> {
    let mut listed: Vec<Scheduled> = lock().get(&client).cloned().unwrap_or_default();
    listed.sort_by_key(|scheduled| scheduled.next);
    listed
}

pub fn take_due(client: u64, now: i64) -> Vec<Scheduled> {
    let mut all = lock();
    let Some(mine) = all.get_mut(&client) else { return Vec::new() };
    let (due, later) = std::mem::take(mine).into_iter().partition(|scheduled| scheduled.next <= now);
    *mine = later;
    due
}

pub fn forget_client(client: u64) {
    lock().remove(&client);
}

pub fn now() -> i64 {
    Timestamp::now().as_second()
}

fn minute_in(zone: &TimeZone, secs: i64) -> Option<String> {
    let at = Timestamp::from_second(secs).ok()?.to_zoned(zone.clone());
    Some(at.strftime("%Y-%m-%d %H:%M").to_string())
}

fn shown(scheduled: &Scheduled) -> String {
    let at = minute_in(&TimeZone::system(), scheduled.next);
    format!("{}, at {}: {}", scheduled.id, at.unwrap_or_default(), scheduled.prompt)
}

#[cfg(test)]
mod tests;

mod tools;
pub use tools::ScheduleWakeup;

pub fn tools() -> Vec<Box<dyn Tool>> {
    vec![Box::new(ScheduleWakeup)]
}
