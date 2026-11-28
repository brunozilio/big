use domain::connection::ConnectionId;
use serde_json::{Value, json};

use super::fixture::{ACCOUNTS, Case, Expect, Fixture, Given};
use super::mock::Mock;
use crate::error::{ActionError, ErrorClass};
use crate::provider::{Credentials, Provider};

const KEY: &str = "conformance";
const TYPED: [(u16, &str); 2] = [(401, "auth"), (503, "transient")];

pub fn class_name(class: &ErrorClass) -> &'static str {
    match class {
        ErrorClass::Transient => "transient",
        ErrorClass::RateLimited { .. } => "rate_limited",
        ErrorClass::Auth => "auth",
        ErrorClass::Conflict => "conflict",
        ErrorClass::Permanent => "permanent",
    }
}

fn creds_of(given: &Given) -> Credentials {
    let settings = if given.settings.is_null() { json!({}) } else { given.settings.clone() };
    Credentials::new(&ConnectionId::from(KEY), &given.token, settings)
}

async fn called(
    provider: &dyn Provider,
    http: &Mock,
    creds: &Credentials,
    action: &str,
    args: &Value,
) -> Result<Value, ActionError> {
    if action == ACCOUNTS {
        let found = provider.accounts(http, creds).await?;
        return serde_json::to_value(found).map_err(ActionError::decode);
    }
    let args = if args.is_null() { json!({}) } else { args.clone() };
    provider.call(http, creds, action, &args, KEY).await
}

fn judged(provider: &dyn Provider, action: &str, answer: &Result<Value, ActionError>, expect: &Expect) -> Vec<String> {
    let mut found = Vec::new();
    match (answer, &expect.ok, &expect.error) {
        (Ok(value), Some(wanted), _) => {
            if !super::fixture::contains(value, wanted) {
                found.push(format!("answered {value}, expected it to contain {wanted}"));
            }
            let schema = provider.descriptor().action(action).map(|spec| spec.output_schema.clone());
            if let Some(schema) = schema.filter(|schema| !schema.is_null())
                && let Err(why) = crate::descriptor::conforms(&schema, value)
            {
                found.push(format!("the answer does not follow the output_schema: {why}"));
            }
        }
        (Ok(value), None, Some(class)) => found.push(format!("answered {value}, expected a {class} error")),
        (Err(error), Some(_), _) => found.push(format!("failed ({}): {}", class_name(&error.class), error.message)),
        (Err(error), None, Some(class)) if class_name(&error.class) != class => {
            found.push(format!("failed as {}, expected {class}: {}", class_name(&error.class), error.message));
        }
        _ => {}
    }
    found
}

fn offline(http: &Mock) -> Vec<String> {
    http.unanswered().into_iter().map(|request| format!("sent a request no mock answers: {request}")).collect()
}

fn cursor_of(answer: &Result<Value, ActionError>, pointer: &str) -> Option<Value> {
    let found = answer.as_ref().ok()?.pointer(pointer)?;
    let empty = found.is_null() || found.as_str().is_some_and(str::is_empty);
    (!empty).then(|| found.clone())
}

pub async fn run(provider: &dyn Provider, fixture: &Fixture, case: &Case) -> Vec<String> {
    let creds = creds_of(case.creds.as_ref().unwrap_or(&fixture.creds));
    let http = Mock::of(&case.http);
    let answer = called(provider, &http, &creds, &case.action, &case.args).await;
    let mut found = judged(provider, &case.action, &answer, &case.expect);
    found.extend(offline(&http));
    let sent = http.sent();
    for wanted in &case.sent {
        if !sent.iter().any(|request| request.contains(wanted.as_str())) {
            found.push(format!("never sent {wanted}; sent {sent:?}"));
        }
    }
    if let Some(next) = &case.next {
        found.extend(paged(provider, &creds, case, next, &answer).await);
    }
    if case.expect.ok.is_some() && !case.http.is_empty() {
        found.extend(typed(provider, &creds, case, fixture.handled(&case.action)).await);
    }
    found
}

async fn paged(
    provider: &dyn Provider,
    creds: &Credentials,
    case: &Case,
    next: &super::fixture::Next,
    first: &Result<Value, ActionError>,
) -> Vec<String> {
    let Some(cursor) = cursor_of(first, &next.cursor) else {
        return vec![format!("the first page carries no cursor at {}", next.cursor)];
    };
    let mut args = if case.args.is_object() { case.args.clone() } else { json!({}) };
    args[next.arg.as_str()] = cursor;
    let http = Mock::of(&next.http);
    let answer = called(provider, &http, creds, &case.action, &args).await;
    let mut found: Vec<String> = judged(provider, &case.action, &answer, &next.expect)
        .into_iter()
        .map(|why| format!("next page: {why}"))
        .collect();
    found.extend(offline(&http));
    if answer.is_ok() && cursor_of(&answer, &next.cursor).is_some() {
        found.push(format!("the last page still carries a cursor at {}", next.cursor));
    }
    found
}

async fn typed(provider: &dyn Provider, creds: &Credentials, case: &Case, handled: &[u16]) -> Vec<String> {
    let mut found = Vec::new();
    for (status, class) in TYPED.into_iter().filter(|(status, _)| !handled.contains(status)) {
        let http = Mock::failing(&case.http, status);
        let answer = called(provider, &http, creds, &case.action, &case.args).await;
        match answer {
            Ok(value) => found.push(format!("with every answer at {status} it still answered {value}")),
            Err(error) if class_name(&error.class) != class => found.push(format!(
                "with every answer at {status} it failed as {}, expected {class}: {}",
                class_name(&error.class),
                error.message
            )),
            Err(_) => {}
        }
    }
    found
}
