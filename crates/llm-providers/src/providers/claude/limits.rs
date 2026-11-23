use domain::billing::PlanUsage;
use serde_json::Value;

use llm_providers::catalog::Call;
use llm_providers::limits::{instant, json_headers, lasting, number, window};
use llm_providers::{Config, Provider as Shape, ProviderError};

pub const USAGE: &str = "https://api.anthropic.com/api/oauth/usage";

const HOURS_5: i64 = 18_000;
const WEEK: i64 = 604_800;

const WINDOWS: [(&str, &str, i64); 4] = [
    ("five_hour", "Sessão (5 h)", HOURS_5),
    ("seven_day", "Semana", WEEK),
    ("seven_day_opus", "Semana (Opus)", WEEK),
    ("seven_day_sonnet", "Semana (Sonnet)", WEEK),
];

pub fn call(provider: &dyn Shape, config: &Config) -> Call {
    Call { url: USAGE.to_owned(), headers: json_headers(config.headers(provider)), body: None }
}

pub fn windows(value: &Value) -> Result<PlanUsage, ProviderError> {
    if !value.is_object() {
        return Err(ProviderError::Invalid("Claude usage is not an object".into()));
    }
    let windows = WINDOWS
        .iter()
        .filter_map(|(id, label, length)| {
            let entry = &value[*id];
            let used = number(&entry["utilization"])?;
            let resets = entry["resets_at"].as_str().and_then(instant);
            Some(lasting(window(id, (*label).to_owned(), used, resets), *length))
        })
        .collect();
    Ok(PlanUsage { plan: None, windows })
}

#[cfg(test)]
#[path = "limits_tests.rs"]
mod tests;
