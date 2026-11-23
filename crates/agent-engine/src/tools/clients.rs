use std::sync::atomic::{AtomicU64, Ordering};

use domain::PermissionMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Waiting {
    Idle,
    Due(String),
    Later(i64),
}

pub fn fresh() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub fn release(client: u64) {
    super::peers::forget(client);
    super::builtin::shell::shells::kill_client(client);
    super::builtin::shell::savings::forget(client);
    super::builtin::schedules::forget_client(client);
    super::watch::forget_client(client);
    crate::team::lead_inbox::closing(client);
    super::inbox::forget_client(client);
    super::hosted::forget(client);
}

pub fn waiting(client: u64, mode: PermissionMode, now: i64) -> Waiting {
    let (notices, _) = domain::inbox::admitted(super::inbox::take(client), mode);
    let due = super::builtin::schedules::take_due(client, now);
    let mut parts: Vec<String> = Vec::new();
    if !notices.is_empty() {
        parts.push(domain::inbox::as_message(&notices));
    }
    parts.extend(due.into_iter().map(|scheduled| scheduled.prompt));
    if !parts.is_empty() {
        return Waiting::Due(parts.join("\n\n"));
    }
    let next = super::builtin::schedules::list(client).first().map(|scheduled| scheduled.next);
    match next {
        Some(at) => Waiting::Later(at),
        None if crate::team::lead_inbox::teammates_working(client) => Waiting::Later(now + 1),
        None => Waiting::Idle,
    }
}

#[cfg(test)]
mod tests {
    use domain::inbox::Notice;

    use domain::schedule::When;

    use super::*;
    use crate::tools::builtin::schedules;

    #[test]
    fn each_client_is_new() {
        assert_ne!(fresh(), fresh());
    }

    #[test]
    fn a_client_with_nothing_pending_is_idle() {
        assert_eq!(waiting(fresh(), PermissionMode::Ask, 100), Waiting::Idle);
    }

    #[test]
    fn mail_wakes_the_lead_and_a_wakeup_waits_for_its_time() {
        let client = fresh();
        crate::tools::inbox::push(client, Notice::new("teammate tester", "terminei"));
        let Waiting::Due(text) = waiting(client, PermissionMode::Ask, 100) else { panic!("mail is due") };
        assert!(text.contains("terminei"));
        schedules::add(client, "olhe o deploy", When::At(500), 100).expect("scheduled");
        assert_eq!(waiting(client, PermissionMode::Ask, 100), Waiting::Later(500));
        assert_eq!(waiting(client, PermissionMode::Ask, 500), Waiting::Due("olhe o deploy".to_owned()));
        assert_eq!(waiting(client, PermissionMode::Ask, 501), Waiting::Idle);
    }

    #[test]
    fn a_released_client_leaves_nothing_behind() {
        let client = fresh();
        schedules::add(client, "depois", When::At(900), 100).expect("scheduled");
        crate::tools::inbox::push(client, Notice::new("teammate tester", "oi"));
        release(client);
        assert_eq!(waiting(client, PermissionMode::Ask, 100), Waiting::Idle);
    }
}
