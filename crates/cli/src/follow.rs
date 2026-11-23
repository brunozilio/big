use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use console::Live;
use domain::SessionId;
use domain::catalog::Listed;

const EVERY: Duration = Duration::from_secs(1);

pub fn run(client: &app::Client, live: &Live, cwd: &Path) {
    let mut sent = BTreeMap::new();
    loop {
        tick(client, live, cwd, &mut sent);
        std::thread::sleep(EVERY);
    }
}

fn tick(client: &app::Client, live: &Live, cwd: &Path, sent: &mut BTreeMap<String, String>) {
    let watched = live.watched().filter(|id| !sent.contains_key(id));
    for (session, text) in gathered(client, live, cwd, watched) {
        if sent.get(&session) != Some(&text) {
            live.send(&session, &text);
            sent.insert(session, text);
        }
    }
}

fn gathered(client: &app::Client, live: &Live, cwd: &Path, watched: Option<String>) -> Vec<(String, String)> {
    let listed = client.sessions().unwrap_or_default();
    live.list(crate::board::shown(&listed, None).to_string());
    let mut sessions: Vec<String> = crate::board::active(&listed).map(|entry| entry.session.0.clone()).collect();
    sessions.extend(watched);
    sessions.sort();
    sessions.dedup();
    let folder = |id: &str| folder_of(&listed, id).unwrap_or_else(|| cwd.to_path_buf());
    let traces = sessions.iter().filter_map(|id| {
        let runs = client.followed_trace(&SessionId(id.clone()), &folder(id)).ok()?;
        Some((id.clone(), runs.to_string()))
    });
    traces.collect()
}

fn folder_of(listed: &[Listed], id: &str) -> Option<PathBuf> {
    listed.iter().find(|entry| entry.session.0 == id).map(|entry| entry.cwd.clone())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use agent_engine::fake::FakeEngine;

    use super::*;

    #[test]
    fn each_tick_gives_the_page_the_session_list() {
        let data = tempfile::tempdir().expect("data");
        let engine = FakeEngine::new(vec![app::Model::new("keyed:scripted", "Scripted")], Vec::new());
        let mut services = app::LocalServices::with_engine(Arc::new(engine));
        services.homes = Arc::new(app::OwnerHome::at(data.path()));
        let client = app::Client::builder().data_dir(data.path()).services(services).build().expect("client");
        let live = Live::default();

        tick(&client, &live, data.path(), &mut BTreeMap::new());

        assert!(live.folder_of("nowhere").is_none());
    }
}
