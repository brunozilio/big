#![allow(unsafe_code)]

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Clock {
    origin: Instant,
}

impl Clock {
    pub(crate) fn start() -> Self {
        Self { origin: Instant::now() }
    }

    pub(crate) fn running(self) -> Duration {
        on_this_thread().unwrap_or_else(|| self.origin.elapsed())
    }
}

#[cfg(unix)]
fn on_this_thread() -> Option<Duration> {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    let asked = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut now) };
    let seconds = u64::try_from(now.tv_sec).ok()?;
    let nanos = u32::try_from(now.tv_nsec).ok()?;
    (asked == 0).then(|| Duration::new(seconds, nanos))
}

#[cfg(not(unix))]
fn on_this_thread() -> Option<Duration> {
    None
}

#[cfg(all(test, unix))]
mod tests {
    use std::time::Duration;

    use super::Clock;

    #[test]
    fn a_thread_that_sleeps_is_not_running_and_one_that_works_is() {
        let clock = Clock::start();
        let before = clock.running();
        std::thread::sleep(Duration::from_millis(500));
        let slept = clock.running().saturating_sub(before);
        assert!(slept < Duration::from_millis(100), "{slept:?}");
        let mut sum = 0u64;
        while clock.running() <= before + slept {
            sum = sum.wrapping_add(std::hint::black_box(sum) | 1);
        }
        std::hint::black_box(sum);
    }
}
