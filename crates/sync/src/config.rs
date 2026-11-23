use std::time::Duration;

use domain::effect::RetryPolicy;

#[derive(Clone, Debug, PartialEq)]
pub struct SyncConfig {
    pub fast_poll: Duration,
    pub slow_poll: Duration,
    pub reconcile_every: Duration,
    pub retry: RetryPolicy,
}

fn number(text: &str, fallback: u64) -> u64 {
    text.trim().parse().ok().filter(|value| *value > 0).unwrap_or(fallback)
}

impl SyncConfig {
    pub fn from_brand() -> SyncConfig {
        SyncConfig::from_values(
            brand::sync::FAST_POLL_SECONDS,
            brand::sync::SLOW_POLL_SECONDS,
            brand::sync::RECONCILE_MINUTES,
            brand::sync::ERROR_BACKOFF_SECONDS,
        )
    }

    pub fn from_values(fast: &str, slow: &str, reconcile_minutes: &str, backoff: &str) -> SyncConfig {
        let fast_poll = Duration::from_secs(number(fast, 15));
        let slow_poll = Duration::from_secs(number(slow, 180)).max(fast_poll);
        let retry = RetryPolicy {
            max: u32::MAX,
            initial: Duration::from_secs(number(backoff, 5)),
            max_interval: slow_poll,
            ..RetryPolicy::default()
        };
        SyncConfig {
            fast_poll,
            slow_poll,
            reconcile_every: Duration::from_secs(number(reconcile_minutes, 10) * 60),
            retry,
        }
    }
}

impl Default for SyncConfig {
    fn default() -> SyncConfig {
        SyncConfig::from_brand()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_file_sets_every_interval() {
        let config = SyncConfig::from_brand();
        assert!(config.fast_poll <= config.slow_poll);
        assert!(config.reconcile_every >= Duration::from_secs(60));
        assert_eq!(config.retry.max_interval, config.slow_poll);
    }

    #[test]
    fn broken_values_fall_back_and_slow_is_never_faster_than_fast() {
        let config = SyncConfig::from_values("x", "1", "0", "");
        assert_eq!(config.fast_poll, Duration::from_secs(15));
        assert_eq!(config.slow_poll, Duration::from_secs(15));
        assert_eq!(config.reconcile_every, Duration::from_secs(600));
        assert_eq!(config.retry.initial, Duration::from_secs(5));
    }
}
