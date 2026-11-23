use std::time::Duration;

use serde_json::json;

use super::*;

fn policy() -> RetryPolicy {
    RetryPolicy {
        max: 4,
        initial: Duration::from_secs(1),
        max_interval: Duration::from_secs(5),
        multiplier: 2.0,
        jitter: 0.5,
    }
}

#[test]
fn transient_errors_back_off_exponentially_up_to_the_cap() {
    let delays: Vec<_> = (1..=3).map(|attempt| next_retry(&policy(), attempt, &ErrorClass::Transient)).collect();
    assert_eq!(delays, [Some(Duration::from_secs(1)), Some(Duration::from_secs(2)), Some(Duration::from_secs(4))]);
    let capped = RetryPolicy { max: 10, ..policy() };
    assert_eq!(next_retry(&capped, 6, &ErrorClass::Transient), Some(Duration::from_secs(5)));
}

#[test]
fn no_retry_once_the_attempts_are_spent() {
    assert_eq!(next_retry(&policy(), 4, &ErrorClass::Transient), None);
    assert_eq!(next_retry(&policy(), 9, &ErrorClass::Conflict), None);
}

#[test]
fn rate_limits_wait_exactly_what_the_provider_asked() {
    let class = ErrorClass::RateLimited { retry_after: Duration::from_secs(42) };
    assert_eq!(next_retry(&policy(), 1, &class), Some(Duration::from_secs(42)));
}

#[test]
fn auth_and_permanent_errors_never_retry() {
    assert_eq!(next_retry(&policy(), 1, &ErrorClass::Auth), None);
    assert_eq!(next_retry(&policy(), 1, &ErrorClass::Permanent), None);
}

#[test]
fn conflicts_retry_at_once_after_reconciling() {
    assert_eq!(next_retry(&policy(), 1, &ErrorClass::Conflict), Some(Duration::ZERO));
}

#[test]
fn attempt_zero_is_treated_as_the_first() {
    assert_eq!(next_retry(&policy(), 0, &ErrorClass::Transient), Some(Duration::from_secs(1)));
}

#[test]
fn jitter_stays_within_the_band_and_is_deterministic_for_a_roll() {
    let base = Duration::from_secs(4);
    assert_eq!(policy().jittered(base, 0.0), Duration::from_secs(2));
    assert_eq!(policy().jittered(base, 1.0), Duration::from_secs(6));
    assert_eq!(policy().jittered(base, 0.5), base);
    assert_eq!(RetryPolicy { jitter: 0.0, ..policy() }.jittered(base, 0.9), base);
    assert_eq!(policy().jittered(base, 7.0), Duration::from_secs(6));
}

#[test]
fn the_idempotency_key_reads_item_step_branch_seq_attempt() {
    let key = IdempotencyKey::new("it-1", "checks", Some("lint"), 3, 2);
    assert_eq!(key.to_string(), "it-1/checks/lint/3/2");
    assert_eq!(IdempotencyKey::new("it-1", "plan", None, 0, 1).to_string(), "it-1/plan//0/1");
}

#[test]
fn effects_round_trip_through_json_with_a_type_tag() {
    let call = Call {
        capability: "tracker".into(),
        action: "transition".into(),
        connection: None,
        args: json!({"to": "Doing"}),
    };
    let effects = [
        Effect::AgentRun { agent: "any".into(), input: json!({}) },
        Effect::Exec { command: "make".into(), cwd: None },
        Effect::ConnectorCall(call.clone()),
        Effect::WaitHuman { view: json!([]) },
        Effect::WaitPoll { call, every: Duration::from_millis(1500) },
        Effect::WaitEvent { kind: "x.y".into(), filter: json!({}), timeout: Some(Duration::from_secs(1)) },
        Effect::Sleep { duration: Duration::from_secs(3) },
        Effect::Child { workflow: "w".into(), input: json!(1) },
        Effect::StatusSet { name: "Any".into() },
        Effect::ImpedimentSet { on: true, reason: Some("r".into()) },
    ];
    for effect in effects {
        let text = serde_json::to_value(&effect).unwrap_or_default();
        assert!(text.get("type").is_some(), "{text}");
        assert_eq!(serde_json::from_value::<Effect>(text).ok(), Some(effect));
    }
}

#[test]
fn durations_are_written_as_milliseconds() {
    let effect = Effect::Sleep { duration: Duration::from_millis(1500) };
    assert_eq!(serde_json::to_value(&effect).ok(), Some(json!({"type": "sleep", "duration": 1500})));
}

#[test]
fn a_request_carries_its_key_retry_and_timeout() {
    let request = EffectRequest::new(
        IdempotencyKey::new("i", "s", None, 1, 1),
        Effect::Sleep { duration: Duration::from_secs(1) },
    )
    .with_timeout(Duration::from_secs(9));
    assert_eq!(request.retry, RetryPolicy::default());
    assert_eq!(request.timeout, Some(Duration::from_secs(9)));
    let back: Option<EffectRequest> = serde_json::to_value(&request).ok().and_then(|v| serde_json::from_value(v).ok());
    assert_eq!(back, Some(request));
}

#[test]
fn outcomes_say_whether_they_succeeded() {
    assert!(EffectOutcome::Ok { value: json!(1) }.is_ok());
    let failed = EffectOutcome::Failed { class: ErrorClass::Auth, message: "expired".into() };
    assert!(!failed.is_ok());
    let back: Option<EffectOutcome> = serde_json::to_value(&failed).ok().and_then(|v| serde_json::from_value(v).ok());
    assert_eq!(back, Some(failed));
}
