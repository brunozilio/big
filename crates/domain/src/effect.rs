mod millis;

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IdempotencyKey {
    pub item: String,
    pub step: String,
    pub branch: Option<String>,
    pub seq: u64,
    pub attempt: u32,
}

impl IdempotencyKey {
    pub fn new(item: &str, step: &str, branch: Option<&str>, seq: u64, attempt: u32) -> Self {
        Self { item: item.to_owned(), step: step.to_owned(), branch: branch.map(str::to_owned), seq, attempt }
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let branch = self.branch.as_deref().unwrap_or_default();
        write!(f, "{}/{}/{}/{}/{}", self.item, self.step, branch, self.seq, self.attempt)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub capability: String,
    pub action: String,
    pub connection: Option<String>,
    pub args: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    AgentRun {
        agent: String,
        input: Value,
    },
    Exec {
        command: String,
        cwd: Option<String>,
    },
    ConnectorCall(Call),
    WaitHuman {
        view: Value,
    },
    WaitPoll {
        call: Call,
        #[serde(with = "millis")]
        every: Duration,
    },
    WaitEvent {
        kind: String,
        filter: Value,
        #[serde(default, with = "millis::optional")]
        timeout: Option<Duration>,
    },
    Sleep {
        #[serde(with = "millis")]
        duration: Duration,
    },
    Child {
        workflow: String,
        input: Value,
    },
    StatusSet {
        name: String,
    },
    ImpedimentSet {
        on: bool,
        reason: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max: u32,
    #[serde(with = "millis")]
    pub initial: Duration,
    #[serde(with = "millis")]
    pub max_interval: Duration,
    pub multiplier: f64,
    pub jitter: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max: 5,
            initial: Duration::from_secs(1),
            max_interval: Duration::from_secs(60),
            multiplier: 2.0,
            jitter: 0.2,
        }
    }
}

impl RetryPolicy {
    pub fn jittered(&self, delay: Duration, roll: f64) -> Duration {
        let spread = self.jitter.clamp(0.0, 1.0) * (2.0 * roll.clamp(0.0, 1.0) - 1.0);
        delay.mul_f64(1.0 + spread)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ErrorClass {
    Transient,
    RateLimited {
        #[serde(with = "millis")]
        retry_after: Duration,
    },
    Auth,
    Conflict,
    Permanent,
}

pub fn next_retry(policy: &RetryPolicy, attempt: u32, class: &ErrorClass) -> Option<Duration> {
    let attempt = attempt.max(1);
    if attempt >= policy.max {
        return None;
    }
    match class {
        ErrorClass::Transient => Some(backoff(policy, attempt)),
        ErrorClass::RateLimited { retry_after } => Some(*retry_after),
        ErrorClass::Conflict => Some(Duration::ZERO),
        ErrorClass::Auth | ErrorClass::Permanent => None,
    }
}

fn backoff(policy: &RetryPolicy, attempt: u32) -> Duration {
    let exponent = i32::try_from(attempt - 1).unwrap_or(i32::MAX);
    let seconds = policy.initial.as_secs_f64() * policy.multiplier.powi(exponent);
    Duration::try_from_secs_f64(seconds).map_or(policy.max_interval, |delay| delay.min(policy.max_interval))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectRequest {
    pub key: IdempotencyKey,
    pub effect: Effect,
    pub retry: RetryPolicy,
    #[serde(default, with = "millis::optional")]
    pub timeout: Option<Duration>,
}

impl EffectRequest {
    pub fn new(key: IdempotencyKey, effect: Effect) -> Self {
        Self { key, effect, retry: RetryPolicy::default(), timeout: None }
    }

    #[must_use]
    pub fn with_retry(self, retry: RetryPolicy) -> Self {
        Self { retry, ..self }
    }

    #[must_use]
    pub fn with_timeout(self, timeout: Duration) -> Self {
        Self { timeout: Some(timeout), ..self }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum EffectOutcome {
    Ok { value: Value },
    Failed { class: ErrorClass, message: String },
    TimedOut,
    Cancelled,
}

impl EffectOutcome {
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok { .. })
    }
}

#[cfg(test)]
mod tests;
