use domain::audit::{CreatorType, FailureStage};
use domain::effect::Call;
use serde_json::Value;

use crate::agent::Failure;
use crate::policy::{Authorized, Gated, surface_of};
use crate::protocol::class_name;

pub fn stage_of(failure: &Failure) -> Option<FailureStage> {
    match failure {
        Failure::Class(class, _) => FailureStage::of_kind(class_name(class)),
        Failure::Blocked(_) => Some(FailureStage::Configuration),
    }
}

fn message_of(failure: &Failure) -> &str {
    match failure {
        Failure::Class(_, message) | Failure::Blocked(message) => message,
    }
}

pub fn recorded(gated: &Gated<'_>, call: &Call, authorized: &Authorized, done: &Result<Value, Failure>) {
    let surface = surface_of(gated.item).word();
    let tag = &authorized.tag;
    let version = tag.version.as_deref();
    let name = tag.friendly_name.as_deref();
    let by = authorized.by.to_string();
    let creator = CreatorType::Automation.word();
    let (user, step, code, wait) = (gated.owner, gated.key.step.as_str(), authorized.code(), authorized.waited_ms);
    match done {
        Ok(_) => tracing::info!(
            target: "audit",
            surface, user, step, capability = %call.capability, connection = call.connection.as_deref(),
            operation.id = %tag.id, operation.version = version, operation.name = name,
            decision.code = code, authorized_by = %by, approval.wait_ms = wait, creator.type = creator,
            outcome = "success",
            "operation.call"
        ),
        Err(failure) => tracing::warn!(
            target: "audit",
            surface, user, step, capability = %call.capability, connection = call.connection.as_deref(),
            operation.id = %tag.id, operation.version = version, operation.name = name,
            decision.code = code, authorized_by = %by, approval.wait_ms = wait, creator.type = creator,
            outcome = "failure", failure.stage = stage_of(failure).map(FailureStage::word), error.message = message_of(failure),
            "operation.call"
        ),
    }
}

#[cfg(test)]
mod tests {
    use domain::effect::ErrorClass;

    use super::*;

    #[test]
    fn a_failure_names_its_stage_from_the_factory_kinds() {
        let stage = |failure: Failure| stage_of(&failure);
        assert_eq!(stage(Failure::Class(ErrorClass::Auth, String::new())), Some(FailureStage::Authentication));
        let limited = ErrorClass::RateLimited { retry_after: std::time::Duration::from_secs(1) };
        assert_eq!(stage(Failure::Class(limited, String::new())), Some(FailureStage::Throttling));
        assert_eq!(stage(Failure::Class(ErrorClass::Conflict, String::new())), Some(FailureStage::Validation));
        assert_eq!(stage(Failure::Class(ErrorClass::Transient, String::new())), Some(FailureStage::Dependency));
        assert_eq!(stage(Failure::Class(ErrorClass::Permanent, String::new())), None);
        assert_eq!(stage(Failure::Blocked("needs a connection".into())), Some(FailureStage::Configuration));
    }
}
