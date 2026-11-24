use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct Degraded {
    reason: Option<Arc<str>>,
}

impl Degraded {
    pub fn because(reason: impl AsRef<str>) -> Self {
        Self { reason: Some(Arc::from(reason.as_ref())) }
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}
