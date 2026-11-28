use std::time::Duration;

use connectors::ErrorClass;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    pub offer: usize,
    pub idle: Duration,
    pub first_wait: Duration,
    pub most_wait: Duration,
}

fn positive(name: &str, text: &str) -> Result<u64, String> {
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0)
        .ok_or_else(|| format!("[audit] {name} must be a whole number above zero, not `{text}`"))
}

fn millis(name: &str, text: &str) -> Result<Duration, String> {
    positive(name, text).map(Duration::from_millis)
}

impl Pace {
    pub fn configured() -> Result<Pace, String> {
        use brand::audit::{FIRST_WAIT_MS, IDLE_MS, MOST_WAIT_MS, OFFER};
        Pace::read(OFFER, IDLE_MS, FIRST_WAIT_MS, MOST_WAIT_MS)
    }

    pub fn read(offer: &str, idle: &str, first_wait: &str, most_wait: &str) -> Result<Pace, String> {
        let offer = usize::try_from(positive("offer", offer)?).map_err(|error| error.to_string())?;
        let pace = Pace {
            offer,
            idle: millis("idle_ms", idle)?,
            first_wait: millis("first_wait_ms", first_wait)?,
            most_wait: millis("most_wait_ms", most_wait)?,
        };
        if pace.first_wait > pace.most_wait {
            return Err("[audit] first_wait_ms cannot be above most_wait_ms".to_owned());
        }
        Ok(pace)
    }

    pub fn backoff(&self, failures: u32) -> Duration {
        let doubled = self.first_wait.saturating_mul(2_u32.saturating_pow(failures.saturating_sub(1)));
        doubled.min(self.most_wait)
    }

    pub fn rate_limited(&self, class: &ErrorClass) -> Option<Duration> {
        match class {
            ErrorClass::RateLimited { retry_after } => Some((*retry_after).min(self.most_wait)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_doubles_from_the_first_up_to_the_most() {
        let pace = Pace::configured().unwrap_or_else(|error| panic!("{error}"));
        let waits: Vec<Duration> = (1..=12).map(|failures| pace.backoff(failures)).collect();
        assert_eq!(waits[0], pace.first_wait);
        assert_eq!(waits[1], pace.first_wait * 2);
        assert!(waits.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(waits.last(), Some(&pace.most_wait));
        let limited = ErrorClass::RateLimited { retry_after: pace.most_wait * 3 };
        assert_eq!(pace.rate_limited(&limited), Some(pace.most_wait));
        assert_eq!(pace.rate_limited(&ErrorClass::Transient), None);
    }

    #[test]
    fn a_pace_that_does_not_read_is_refused() {
        assert!(Pace::read("0", "1", "1", "1").is_err());
        assert!(Pace::read("5", "x", "1", "1").is_err());
        assert!(Pace::read("5", "1", "9", "2").is_err());
        assert_eq!(Pace::read("5", "1", "2", "3").map(|pace| pace.offer), Ok(5));
    }
}
