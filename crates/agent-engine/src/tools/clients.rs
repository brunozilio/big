use std::sync::atomic::{AtomicU64, Ordering};

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

pub fn background(client: u64) -> Vec<(String, String, bool)> {
    super::builtin::shell::shells::list(client)
}

pub fn stop_background(client: u64, id: &str) -> bool {
    let stopped = super::watch::stop(client, id).or_else(|| super::builtin::shell::shells::stopped(client, id));
    let Some(command) = stopped else { return false };
    let source = format!("{}{id}", domain::inbox::BACKGROUND_SOURCE);
    super::inbox::push(client, domain::inbox::Notice::new(source, format!("`{command}` was stopped by the user.")));
    true
}

pub fn waiting(client: u64, now: i64) -> Waiting {
    let notices = super::inbox::take(client);
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

    use super::*;
    use crate::tools::builtin::schedules;

    #[cfg(unix)]
    #[test]
    fn the_user_stops_a_background_command_and_the_agent_is_told() {
        let client = fresh();
        let mut workspace = crate::tools::Workspace::new(std::env::temp_dir());
        workspace.credentials.client = client;
        let mut command = std::process::Command::new("sleep");
        command.arg("30");
        let id = super::super::builtin::shell::shells::start(&workspace, "sleep 30", &mut command).expect("started");
        assert!(background(client).iter().any(|(listed, _, running)| *listed == id && *running));
        assert!(stop_background(client, &id));
        assert!(!background(client).iter().any(|(listed, _, _)| *listed == id));
        let told = super::super::inbox::take(client);
        assert_eq!(told.len(), 1);
        assert_eq!(told[0].ended_background(), Some(id.as_str()));
        assert!(told[0].text.contains("stopped by the user"));
        assert!(!stop_background(client, &id), "a command already stopped is not stopped twice");
        release(client);
    }

    #[test]
    fn each_client_is_new() {
        assert_ne!(fresh(), fresh());
    }

    #[test]
    fn a_client_with_nothing_pending_is_idle() {
        assert_eq!(waiting(fresh(), 100), Waiting::Idle);
    }

    #[test]
    fn mail_wakes_the_lead_and_a_wakeup_waits_for_its_time() {
        let client = fresh();
        crate::tools::inbox::push(client, Notice::new("teammate tester", "terminei"));
        let Waiting::Due(text) = waiting(client, 100) else { panic!("mail is due") };
        assert!(text.contains("terminei"));
        schedules::add(client, "olhe o deploy", 500).expect("scheduled");
        assert_eq!(waiting(client, 100), Waiting::Later(500));
        assert_eq!(waiting(client, 500), Waiting::Due("olhe o deploy".to_owned()));
        assert_eq!(waiting(client, 501), Waiting::Idle);
    }

    #[test]
    fn a_released_client_leaves_nothing_behind() {
        let client = fresh();
        schedules::add(client, "depois", 900).expect("scheduled");
        crate::tools::inbox::push(client, Notice::new("teammate tester", "oi"));
        release(client);
        assert_eq!(waiting(client, 100), Waiting::Idle);
    }
}
