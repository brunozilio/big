use domain::billing::PlanWindow;
use jiff::Timestamp;
use serde_json::Value;

pub const LIMIT: u64 = 1 << 20;

pub fn json_headers(mut headers: Vec<(String, String)>) -> Vec<(String, String)> {
    headers.retain(|(name, _)| !name.eq_ignore_ascii_case("accept"));
    headers.push(("accept".to_owned(), "application/json".to_owned()));
    headers
}

pub fn window(id: &str, label: String, used: f64, resets_at: Option<String>) -> PlanWindow {
    PlanWindow { id: id.to_owned(), label, used_percent: used.clamp(0.0, 100.0), resets_at, starts_at: None }
}

pub fn lasting(mut window: PlanWindow, seconds: i64) -> PlanWindow {
    let reset = window.resets_at.as_deref().and_then(|at| at.parse::<Timestamp>().ok());
    window.starts_at =
        reset.and_then(|at| at.checked_sub(jiff::SignedDuration::from_secs(seconds)).ok()).map(|at| at.to_string());
    window
}

pub fn monthly(mut window: PlanWindow) -> PlanWindow {
    let reset = window.resets_at.as_deref().and_then(|at| at.parse::<Timestamp>().ok());
    let start = reset.and_then(|at| at.to_zoned(jiff::tz::TimeZone::UTC).checked_sub(jiff::Span::new().months(1)).ok());
    window.starts_at = start.map(|at| at.timestamp().to_string());
    window
}

pub fn instant(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let parsed = text.parse::<Timestamp>().or_else(|_| format!("{text}T00:00:00Z").parse::<Timestamp>());
    parsed.ok().map(|at| at.to_string())
}

pub fn epoch(seconds: i64) -> Option<String> {
    Timestamp::from_second(seconds).ok().map(|at| at.to_string())
}

pub fn after(seconds: i64) -> Option<String> {
    epoch(Timestamp::now().as_second().saturating_add(seconds))
}

pub fn number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

pub fn titled(text: &str) -> Option<String> {
    let text = text.trim().replace(['_', '-'], " ");
    let mut chars = text.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

pub fn span(seconds: i64) -> String {
    const HOUR: i64 = 3600;
    const DAY: i64 = 24 * HOUR;
    match seconds {
        18_000 => "Sessão (5 h)".to_owned(),
        604_800 => "Semana".to_owned(),
        DAY => "1 dia".to_owned(),
        s if s > DAY && s % DAY == 0 => format!("{} dias", s / DAY),
        s if s >= HOUR => format!("{} h", s / HOUR),
        s => format!("{} min", (s / 60).max(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instants_come_out_in_utc_whether_given_with_offset_or_as_a_date() {
        assert_eq!(instant("2026-11-23T15:00:00.5+00:00").as_deref(), Some("2026-11-23T15:00:00.5Z"));
        assert_eq!(instant("2026-12-01").as_deref(), Some("2026-12-01T00:00:00Z"));
        assert_eq!(instant("nope"), None);
        assert_eq!(epoch(0).as_deref(), Some("1970-01-01T00:00:00Z"));
    }

    #[test]
    fn a_window_starts_its_length_or_a_calendar_month_before_it_resets() {
        let reset = || window("w", "W".into(), 10.0, Some("2026-03-31T00:00:00Z".into()));
        assert_eq!(lasting(reset(), 18_000).starts_at.as_deref(), Some("2026-03-30T19:00:00Z"));
        assert_eq!(monthly(reset()).starts_at.as_deref(), Some("2026-02-28T00:00:00Z"));
        assert_eq!(lasting(window("w", "W".into(), 1.0, None), 60).starts_at, None);
    }

    #[test]
    fn spans_are_named_by_their_length() {
        assert_eq!(span(18_000), "Sessão (5 h)");
        assert_eq!(span(604_800), "Semana");
        assert_eq!(span(86_400 * 30), "30 dias");
        assert_eq!(span(7200), "2 h");
    }

    #[test]
    fn the_accept_header_asks_for_json() {
        let headers = json_headers(vec![("accept".into(), "text/event-stream".into()), ("x".into(), "y".into())]);
        assert_eq!(headers, [("x".to_owned(), "y".to_owned()), ("accept".to_owned(), "application/json".to_owned())]);
    }
}
