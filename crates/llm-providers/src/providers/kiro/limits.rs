use domain::billing::{PlanUsage, PlanWindow};
use serde_json::{Value, json};

use llm_providers::catalog::Call;
use llm_providers::limits::{epoch, instant, json_headers, monthly, number, titled, window};
use llm_providers::{Config, ProviderError};

pub const TARGET: &str = "AmazonCodeWhispererService.GetUsageLimits";

pub fn call(config: &Config) -> Call {
    let arn = config.account();
    Call {
        url: llm_providers::oauth::regional(brand::kiro::Q_URL, super::region(arn)),
        headers: json_headers(super::headers(config.access_token(), TARGET)),
        body: Some(json!({ "origin": super::ORIGIN, "profileArn": arn }).to_string()),
    }
}

pub fn windows(value: &Value, language: domain::Language) -> Result<PlanUsage, ProviderError> {
    let list = value["usageBreakdownList"]
        .as_array()
        .ok_or_else(|| ProviderError::Invalid("Kiro usage has no usageBreakdownList".into()))?;
    let reset = moment(&value["nextDateReset"]);
    let windows = list.iter().flat_map(|entry| breakdown(entry, reset.as_ref(), language)).collect();
    Ok(PlanUsage { plan: plan(&value["subscriptionInfo"]), windows })
}

fn plan(info: &Value) -> Option<String> {
    let title = info["subscriptionTitle"].as_str().map(str::trim).filter(|t| !t.is_empty());
    title.map(str::to_owned).or_else(|| info["type"].as_str().and_then(titled))
}

fn breakdown(entry: &Value, reset: Option<&String>, language: domain::Language) -> Vec<PlanWindow> {
    let kind = entry["resourceType"].as_str().unwrap_or("usage").to_ascii_lowercase();
    let credits = language.pick("Credits", "Créditos", "Créditos");
    let name = entry["displayNamePlural"].as_str().or_else(|| entry["displayName"].as_str()).unwrap_or(credits);
    let resets = moment(&entry["nextDateReset"]).or_else(|| reset.cloned());
    let mut out: Vec<PlanWindow> =
        metered(&kind, name.to_owned(), entry, resets, language).map(monthly).into_iter().collect();
    let trial = &entry["freeTrialInfo"];
    if trial["freeTrialStatus"].as_str() == Some("ACTIVE") {
        let label = language.pick(
            format!("{name} (free trial)"),
            format!("{name} (prueba gratuita)"),
            format!("{name} (teste gratuito)"),
        );
        out.extend(metered(&format!("{kind}_trial"), label, trial, moment(&trial["freeTrialExpiry"]), language));
    }
    let bonuses = entry["bonuses"].as_array().map(Vec::as_slice).unwrap_or_default();
    out.extend(bonuses.iter().enumerate().filter_map(|(at, bonus)| extra(&kind, name, at, bonus, language)));
    out
}

fn extra(kind: &str, name: &str, at: usize, bonus: &Value, language: domain::Language) -> Option<PlanWindow> {
    if bonus["status"].as_str().is_some_and(|status| status != "ACTIVE") {
        return None;
    }
    let fallback = || language.pick(format!("{name} bonus"), format!("{name} bono"), format!("{name} bônus"));
    let title = bonus["displayName"].as_str().map_or_else(fallback, str::to_owned);
    metered(&format!("{kind}_bonus_{at}"), title, bonus, moment(&bonus["expiresAt"]), language)
}

fn metered(
    id: &str,
    name: String,
    entry: &Value,
    resets: Option<String>,
    language: domain::Language,
) -> Option<PlanWindow> {
    let used = amount(entry, "currentUsage")?;
    let limit = amount(entry, "usageLimit").filter(|limit| *limit > 0.0)?;
    let (spent, most) = (shown(used), shown(limit));
    let label = language.pick(
        format!("{name} · {spent} of {most}"),
        format!("{name} · {spent} de {most}"),
        format!("{name} · {spent} de {most}"),
    );
    Some(window(id, label, used / limit * 100.0, resets))
}

fn amount(entry: &Value, key: &str) -> Option<f64> {
    number(&entry[format!("{key}WithPrecision")]).or_else(|| number(&entry[key]))
}

fn shown(amount: f64) -> String {
    let rounded = (amount * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 { format!("{rounded:.0}") } else { format!("{rounded}") }
}

fn moment(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => instant(text),
        Value::Number(_) => value.as_f64().and_then(|seconds| epoch(seconds.floor() as i64)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "limits_tests.rs"]
mod tests;
