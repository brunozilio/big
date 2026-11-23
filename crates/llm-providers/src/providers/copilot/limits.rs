use domain::billing::{PlanUsage, PlanWindow};
use serde_json::Value;

use llm_providers::catalog::Call;
use llm_providers::limits::{instant, monthly, number, titled, window};
use llm_providers::{Config, ProviderError};

use super::device;

const QUOTAS: [(&str, &str); 3] =
    [("premium_interactions", "Requisições premium"), ("chat", "Chat"), ("completions", "Autocompletar")];

pub fn call(config: &Config) -> Call {
    Call { url: device::USER.to_owned(), headers: device::user_headers(config.access_token()), body: None }
}

pub fn windows(value: &Value) -> Result<PlanUsage, ProviderError> {
    if !value.is_object() {
        return Err(ProviderError::Invalid("the Copilot user is not an object".into()));
    }
    let resets =
        ["quota_reset_date_utc", "quota_reset_date"].iter().find_map(|key| value[*key].as_str().and_then(instant));
    let snapshots = &value["quota_snapshots"];
    let windows = QUOTAS.iter().filter_map(|(id, label)| quota(id, label, &snapshots[*id], resets.clone())).collect();
    Ok(PlanUsage { plan: value["copilot_plan"].as_str().and_then(titled), windows })
}

fn quota(id: &str, label: &str, entry: &Value, resets: Option<String>) -> Option<PlanWindow> {
    if entry["unlimited"] == true {
        return None;
    }
    let remaining = number(&entry["percent_remaining"])?;
    Some(monthly(window(id, label.to_owned(), 100.0 - remaining, resets)))
}

#[cfg(test)]
#[path = "limits_tests.rs"]
mod tests;
