use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::response::Response;
use domain::billing::{AccountLimits, PlanLimits};
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult, json as reply};
use crate::state::{Ctx, Shared};

const ERROR_CHARS: usize = 200;

pub fn listing(accounts: &[AccountLimits], shown: &HashMap<String, (Value, Value)>) -> Value {
    let accounts: Vec<Value> = accounts
        .iter()
        .map(|account| {
            let provider = account.provider.id();
            let (name, logo) = shown.get(provider).cloned().unwrap_or_else(|| (json!(provider), json!(provider)));
            let (status, error, plan, windows) = match &account.limits {
                PlanLimits::Known(usage) => ("ok", None, usage.plan.clone(), windows(&usage.windows)),
                PlanLimits::Unsupported => ("unsupported", None, None, Vec::new()),
                PlanLimits::Failed(message) => {
                    ("error", Some(message.chars().take(ERROR_CHARS).collect::<String>()), None, Vec::new())
                }
            };
            json!({
                "provider": provider, "name": name, "logo": logo, "account": account.account,
                "status": status, "error": error, "plan": plan, "windows": windows,
            })
        })
        .collect();
    json!({ "accounts": accounts })
}

fn windows(windows: &[domain::billing::PlanWindow]) -> Vec<Value> {
    windows
        .iter()
        .map(|window| {
            json!({
                "id": window.id, "label": window.label,
                "usedPercent": window.used_percent, "resetsAt": window.resets_at, "startsAt": window.starts_at,
            })
        })
        .collect()
}

fn shown(shared: &Shared) -> HashMap<String, (Value, Value)> {
    crate::ai::subscriptions(shared)
        .iter()
        .map(|summary| {
            let described = crate::ai::provider_json(summary);
            (summary.provider.id().to_owned(), (described["name"].clone(), described["logo"].clone()))
        })
        .collect()
}

pub async fn list(State(ctx): State<Ctx>) -> ApiResult<Response> {
    let shared = Arc::clone(&ctx);
    let listed = tokio::task::spawn_blocking(move || listing(&shared.backend.client.plan_limits(), &shown(&shared)))
        .await
        .map_err(ApiError::failed)?;
    Ok(reply(listed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ModelProvider;
    use domain::billing::{PlanUsage, PlanWindow};

    #[test]
    fn each_account_says_whether_its_limits_are_known_unsupported_or_failed() {
        let usage = PlanUsage {
            plan: Some("Max".into()),
            windows: vec![PlanWindow {
                id: "five_hour".into(),
                label: "Sessão (5 h)".into(),
                used_percent: 42.0,
                resets_at: Some("2026-11-23T18:00:00Z".into()),
                starts_at: Some("2026-11-23T13:00:00Z".into()),
            }],
        };
        let account = |provider: &'static str, limits| AccountLimits {
            provider: ModelProvider::new(provider),
            account: "a@x".into(),
            limits,
        };
        let accounts = [
            account("claude-subscription", PlanLimits::Known(usage)),
            account("grok-subscription", PlanLimits::Unsupported),
            account("openai-subscription", PlanLimits::Failed("x".repeat(500))),
        ];
        let shown = HashMap::from([("claude-subscription".to_owned(), (json!("Claude"), json!("<svg/>")))]);
        let listed = listing(&accounts, &shown);
        assert_eq!(
            listed["accounts"][0],
            json!({"provider": "claude-subscription", "name": "Claude", "logo": "<svg/>", "account": "a@x",
                "status": "ok", "error": null, "plan": "Max",
                "windows": [{"id": "five_hour", "label": "Sessão (5 h)", "usedPercent": 42.0, "resetsAt": "2026-11-23T18:00:00Z", "startsAt": "2026-11-23T13:00:00Z"}]})
        );
        assert_eq!(listed["accounts"][1]["status"], "unsupported");
        assert_eq!(listed["accounts"][1]["windows"], json!([]));
        assert_eq!(listed["accounts"][2]["status"], "error");
        assert_eq!(listed["accounts"][2]["error"].as_str().map(str::len), Some(ERROR_CHARS));
        assert_eq!(listed["accounts"][2]["name"], "openai-subscription");
    }
}
