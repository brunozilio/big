use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use agent_engine::AgentEngine;
use connectors::Connectors;
use domain::effect::RetryPolicy;
use store::Store;

use crate::protocol::{AGENT, CALL, EXEC};
use crate::resources::Resources;

#[derive(Debug, Clone, PartialEq)]
pub struct Timeouts {
    pub agent: Duration,
    pub exec: Duration,
    pub call: Duration,
}

impl Timeouts {
    pub fn of(&self, kind: &str) -> Option<Duration> {
        match kind {
            AGENT => Some(self.agent),
            EXEC => Some(self.exec),
            CALL => Some(self.call),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    pub max_parallel_items: usize,
    pub retry: RetryPolicy,
    pub timeouts: Timeouts,
    pub ask_timeout: Duration,
}

fn parsed<T: FromStr>(text: &str, fallback: T) -> T {
    text.trim().parse().unwrap_or(fallback)
}

fn millis(text: &str, fallback: Duration) -> Duration {
    text.trim().parse::<u64>().map_or(fallback, Duration::from_millis)
}

impl Limits {
    pub fn from_config() -> Limits {
        let fallback = RetryPolicy::default();
        Limits {
            max_parallel_items: parsed(brand::server::MAX_PARALLEL_ITEMS, 4).max(1),
            retry: RetryPolicy {
                max: parsed(brand::retry::MAX, fallback.max),
                initial: millis(brand::retry::INITIAL_MS, fallback.initial),
                max_interval: millis(brand::retry::MAX_INTERVAL_MS, fallback.max_interval),
                multiplier: parsed(brand::retry::MULTIPLIER, fallback.multiplier),
                jitter: parsed(brand::retry::JITTER, fallback.jitter),
            },
            timeouts: Timeouts {
                agent: millis(brand::timeouts::AGENT_MS, Duration::from_secs(7200)),
                exec: millis(brand::timeouts::EXEC_MS, Duration::from_secs(1800)),
                call: millis(brand::timeouts::CALL_MS, Duration::from_secs(120)),
            },
            ask_timeout: millis(brand::workflow::ASK_TIMEOUT_MS, Duration::from_secs(1800)),
        }
    }
}

#[derive(Clone)]
pub struct Config {
    pub store: Arc<Store>,
    pub connectors: Arc<Connectors>,
    pub engine: Arc<dyn AgentEngine>,
    pub resources: Resources,
    pub repo: Option<PathBuf>,
    pub data: PathBuf,
    pub limits: Limits,
    pub sync: bool,
    pub sync_config: sync::SyncConfig,
}

impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("resources", &self.resources)
            .field("repo", &self.repo)
            .field("data", &self.data)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl Config {
    pub fn new(store: Arc<Store>, connectors: Arc<Connectors>, engine: Arc<dyn AgentEngine>, data: PathBuf) -> Config {
        Config {
            store,
            connectors,
            engine,
            resources: Resources::standard(None),
            repo: None,
            data,
            limits: Limits::from_config(),
            sync: true,
            sync_config: sync::SyncConfig::from_brand(),
        }
    }

    #[must_use]
    pub fn in_repo(mut self, repo: PathBuf) -> Config {
        self.resources = Resources::standard(Some(&repo));
        self.repo = Some(repo);
        self
    }

    #[must_use]
    pub fn with_resources(mut self, resources: Resources) -> Config {
        self.resources = resources;
        self
    }

    #[must_use]
    pub fn without_sync(mut self) -> Config {
        self.sync = false;
        self
    }

    #[must_use]
    pub fn with_sync_config(mut self, sync_config: sync::SyncConfig) -> Config {
        self.sync_config = sync_config;
        self
    }

    #[must_use]
    pub fn with_limits(mut self, limits: Limits) -> Config {
        self.limits = limits;
        self
    }
}
