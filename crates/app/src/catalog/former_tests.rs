use std::path::Path;

use domain::SessionId;

use super::*;
use crate::catalog::{SessionCatalog, SqliteSessionCatalog};

fn former_in(dir: &Path) -> Former {
    Former { name: "Old Name".into(), database: dir.join("old/app.db"), sessions: dir.join("old/native-sessions") }
}

fn keep_former_session(former: &Former, cwd: &Path, id: &SessionId, with_file: bool) {
    std::fs::create_dir_all(&former.sessions).expect("dir");
    let catalog = SqliteSessionCatalog::new(Db::open(&former.database).expect("old db"));
    catalog.update(&mut |kept| kept.started(id, cwd, 5));
    catalog.update(&mut |kept| kept.rename(id, "Gateway"));
    if with_file {
        let stem = scoped_id(cwd, id).0;
        std::fs::write(former.sessions.join(format!("{stem}.jsonl")), "{}\n").expect("log");
        std::fs::write(former.sessions.join(format!("checkpoints-{stem}.json")), "[]").expect("side");
    }
}

#[test]
fn the_sessions_of_a_former_name_are_copied_and_listed_once() {
    let dir = tempfile::tempdir().expect("dir");
    let former = former_in(dir.path());
    let (cwd, kept, empty) = (Path::new("/work/gateway"), SessionId("kept".into()), SessionId("empty".into()));
    keep_former_session(&former, cwd, &kept, true);
    keep_former_session(&former, cwd, &empty, false);
    let (data, sessions) = (dir.path().join("new"), dir.path().join("new/native-sessions"));
    std::fs::create_dir_all(&data).expect("data");
    let db = Db::open(&data.join("app.db")).expect("new db");
    import(&db, &data, &sessions, std::slice::from_ref(&former));
    let catalog = SqliteSessionCatalog::new(db.clone());
    let listed = catalog.read();
    assert_eq!(listed.listed().len(), 1, "a session with no conversation kept is left out");
    assert_eq!(listed.listed()[0].title.as_deref(), Some("Gateway"));
    let stem = scoped_id(cwd, &kept).0;
    assert!(sessions.join(format!("{stem}.jsonl")).is_file());
    assert!(sessions.join(format!("checkpoints-{stem}.json")).is_file());
    assert!(former.sessions.join(format!("{stem}.jsonl")).is_file(), "the former copy stays");
    catalog.update(&mut |all| all.delete(&kept));
    import(&db, &data, &sessions, std::slice::from_ref(&former));
    assert!(catalog.read().listed().is_empty(), "a session deleted after the import does not come back");
}

#[test]
fn a_former_name_with_no_database_is_passed_over() {
    let dir = tempfile::tempdir().expect("dir");
    let db = Db::open(&dir.path().join("app.db")).expect("db");
    import(&db, dir.path(), &dir.path().join("s"), &[former_in(dir.path())]);
    assert!(SqliteSessionCatalog::new(db).read().listed().is_empty());
    assert!(!dir.path().join("imported-from-old-name").exists());
}
