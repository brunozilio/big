use domain::billing::{PlanUsage, PlanWindow};
use serde_json::Value;

use llm_providers::catalog::Call;
use llm_providers::limits::{after, epoch, json_headers, lasting, number, span, titled, window};
use llm_providers::{Config, Provider as Shape, ProviderError};

pub const PATH: &str = "/wham/usage";

const WINDOWS: [(&str, &str); 2] = [("primary_window", "primary"), ("secondary_window", "secondary")];

pub fn call(provider: &dyn Shape, config: &Config) -> Call {
    let call = Call::get(provider, config, PATH);
    Call { headers: json_headers(call.headers), ..call }
}

pub fn windows(value: &Value, language: domain::Language) -> Result<PlanUsage, ProviderError> {
    if !value.is_object() {
        return Err(ProviderError::Invalid("ChatGPT usage is not an object".into()));
    }
    let limits = &value["rate_limit"];
    let windows = WINDOWS.iter().filter_map(|(key, id)| limit(id, &limits[*key], language)).collect();
    Ok(PlanUsage { plan: value["plan_type"].as_str().and_then(titled), windows })
}

fn limit(id: &str, entry: &Value, language: domain::Language) -> Option<PlanWindow> {
    let used = number(&entry["used_percent"])?;
    let length = entry["limit_window_seconds"].as_i64().filter(|s| *s > 0);
    let label = length.map_or_else(|| id.to_owned(), |seconds| span(seconds, language));
    let resets =
        entry["reset_at"].as_i64().and_then(epoch).or_else(|| entry["reset_after_seconds"].as_i64().and_then(after));
    let shaped = window(id, label, used, resets);
    Some(match length {
        Some(seconds) => lasting(shaped, seconds),
        None => shaped,
    })
}

#[cfg(test)]
#[path = "limits_tests.rs"]
mod tests;
