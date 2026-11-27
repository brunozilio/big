use std::time::Duration;

use domain::effect::RetryPolicy;

#[derive(Clone, Debug, PartialEq)]
pub struct SyncConfig {
    pub fast_poll: Duration,
    pub slow_poll: Duration,
    pub reconcile_every: Duration,
    pub seen_retention: Duration,
    pub ended_retention: Option<Duration>,
    pub retry: RetryPolicy,
}

const DAY: u64 = 24 * 60 * 60;

fn number(text: &str, fallback: u64) -> u64 {
    text.trim().parse().ok().filter(|value| *value > 0).unwrap_or(fallback)
}

impl SyncConfig {
    pub fn from_brand() -> SyncConfig {
        SyncConfig {
            ended_retention: ended_retention(brand::store::ENDED_RETENTION_DAYS),
            ..SyncConfig::from_values(
                brand::sync::FAST_POLL_SECONDS,
                brand::sync::SLOW_POLL_SECONDS,
                brand::sync::RECONCILE_MINUTES,
                brand::sync::ERROR_BACKOFF_SECONDS,
                brand::sync::SEEN_RETENTION_DAYS,
            )
        }
    }

    pub fn from_values(fast: &str, slow: &str, reconcile_minutes: &str, backoff: &str, seen_days: &str) -> SyncConfig {
        let fast_poll = Duration::from_secs(number(fast, 15));
        let slow_poll = Duration::from_secs(number(slow, 180)).max(fast_poll);
        let retry = RetryPolicy {
            max: u32::MAX,
            initial: Duration::from_secs(number(backoff, 5)),
            max_interval: slow_poll,
            ..RetryPolicy::default()
        };
        let reconcile_every = Duration::from_secs(number(reconcile_minutes, 10) * 60);
        let seen_retention =
            Duration::from_secs(number(seen_days, 30).saturating_mul(DAY)).max(slow_poll.max(reconcile_every) * 2);
        SyncConfig { fast_poll, slow_poll, reconcile_every, seen_retention, ended_retention: None, retry }
    }
}

pub fn ended_retention(days: &str) -> Option<Duration> {
    let days = days.trim().parse::<u64>().ok().filter(|days| *days > 0)?;
    Some(Duration::from_secs(days.saturating_mul(DAY)))
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
        assert!(config.seen_retention >= Duration::from_secs(DAY));
    }

    #[test]
    fn broken_values_fall_back_and_slow_is_never_faster_than_fast() {
        let config = SyncConfig::from_values("x", "1", "0", "", "?");
        assert_eq!(config.fast_poll, Duration::from_secs(15));
        assert_eq!(config.slow_poll, Duration::from_secs(15));
        assert_eq!(config.reconcile_every, Duration::from_secs(600));
        assert_eq!(config.retry.initial, Duration::from_secs(5));
        assert_eq!(config.seen_retention, Duration::from_secs(30 * DAY));
    }

    #[test]
    fn ended_items_are_kept_forever_unless_a_number_of_days_is_set() {
        assert_eq!(SyncConfig::from_brand().ended_retention, ended_retention(brand::store::ENDED_RETENTION_DAYS));
        assert_eq!(ended_retention("0"), None);
        assert_eq!(ended_retention(""), None);
        assert_eq!(ended_retention("x"), None);
        assert_eq!(ended_retention("2"), Some(Duration::from_secs(2 * DAY)));
    }

    #[test]
    fn seen_events_outlive_every_poll_and_reconcile_interval() {
        let config = SyncConfig::from_values("15", "600000", "100000", "5", "1");
        assert_eq!(config.seen_retention, config.reconcile_every * 2);
        assert!(config.seen_retention >= config.slow_poll * 2);
    }
}
