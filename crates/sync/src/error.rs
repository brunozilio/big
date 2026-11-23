use connectors::ActionError;
use domain::effect::ErrorClass;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("store: {0}")]
    Store(#[from] store::Error),
    #[error("connector: {0}")]
    Action(#[from] ActionError),
    #[error("sink: {0}")]
    Sink(String),
}

impl Error {
    pub fn class(&self) -> ErrorClass {
        match self {
            Error::Action(error) => class_of(&error.class),
            Error::Store(_) | Error::Sink(_) => ErrorClass::Transient,
        }
    }

    pub fn is_auth(&self) -> bool {
        self.class() == ErrorClass::Auth
    }
}

pub fn class_of(class: &connectors::ErrorClass) -> ErrorClass {
    match class {
        connectors::ErrorClass::Transient => ErrorClass::Transient,
        connectors::ErrorClass::RateLimited { retry_after } => ErrorClass::RateLimited { retry_after: *retry_after },
        connectors::ErrorClass::Auth => ErrorClass::Auth,
        connectors::ErrorClass::Conflict => ErrorClass::Conflict,
        connectors::ErrorClass::Permanent => ErrorClass::Permanent,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn connector_errors_keep_their_class_and_the_rest_is_transient() {
        let limited = Error::from(ActionError::rate_limited(Duration::from_secs(3), "slow"));
        assert_eq!(limited.class(), ErrorClass::RateLimited { retry_after: Duration::from_secs(3) });
        assert!(Error::from(ActionError::reconnect()).is_auth());
        assert_eq!(Error::from(ActionError::conflict("x")).class(), ErrorClass::Conflict);
        assert_eq!(Error::from(ActionError::permanent("x")).class(), ErrorClass::Permanent);
        assert_eq!(Error::Sink("down".into()).class(), ErrorClass::Transient);
    }
}
