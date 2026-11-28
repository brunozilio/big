use std::time::{Duration, Instant};

use crossbeam_channel::unbounded;
use database::Value;

use super::schema::MIGRATIONS;
use super::testing::{Place, on_postgres};
use super::*;

const NOTES: &str = "CREATE TABLE notes (text TEXT NOT NULL)";
const UNHEARD_OF: &str = "CREATE TABLE unheard_of (x INTEGER)";
const SHORT: Duration = Duration::from_millis(150);

fn version(db: &Db) -> i64 {
    db.read(|conn| conn.one("SELECT COUNT(*) FROM schema_version", &[])).expect("version").unwrap_or(0)
}

fn tables(db: &Db) -> Vec<String> {
    let listed = match db.kept.database.dialect() {
        Dialect::Sqlite => "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
        Dialect::Postgres => {
            "SELECT table_name FROM information_schema.tables WHERE table_schema = current_schema() ORDER BY table_name"
        }
    };
    db.read(|conn| conn.all(listed, &[])).expect("tables")
}

fn pragma(db: &Db, name: &str) -> String {
    let value = db.kept.database.sqlite_connection(|connection| {
        connection.query_row(&format!("PRAGMA {name}"), [], |row| row.get::<_, rusqlite::types::Value>(0))
    });
    match value.expect("sqlite").expect("pragma") {
        rusqlite::types::Value::Text(text) => text,
        rusqlite::types::Value::Integer(number) => number.to_string(),
        other => format!("{other:?}"),
    }
}

fn noted(db: &Db, text: &str) {
    db.write(|conn| conn.execute("INSERT INTO notes (text) VALUES (?1)", args![text])).expect("noted");
}

fn notes(db: &Db) -> i64 {
    db.read(|conn| conn.one("SELECT COUNT(*) FROM notes", &[])).expect("notes").unwrap_or(0)
}

fn notes_db(place: &Place) -> Db {
    place.opened(WAIT, &[NOTES]).expect("open")
}

fn holds_every_table_at_the_newest_version(place: &Place) {
    let db = place.open();

    assert_eq!(version(&db), 1);
    assert_eq!(tables(&db), ["memories", "memory_books", "memory_undo_points", "schema_version", "sessions"]);
}

fn opening_again_changes_nothing(place: &Place) {
    let first = notes_db(place);
    noted(&first, "kept");
    drop(first);

    let again = notes_db(place);
    let beside = notes_db(place);

    assert_eq!((version(&again), notes(&again), notes(&beside)), (1, 1, 1));
}

fn only_unseen_migrations_are_applied(place: &Place) {
    noted(&notes_db(place), "from the first version");

    let later = place.opened(WAIT, &[NOTES, "ALTER TABLE notes ADD COLUMN at INTEGER"]).expect("later");

    assert_eq!((version(&later), notes(&later)), (2, 1));
    later.write(|conn| conn.execute("INSERT INTO notes (text, at) VALUES ('new', 7)", &[])).expect("at");
}

fn a_failed_migration_leaves_the_version(place: &Place) {
    drop(notes_db(place));

    let failed = place.opened(WAIT, &[NOTES, "CREATE TABLE more (x INTEGER); CREATE TABLE notes (y INTEGER)"]);

    assert!(matches!(failed, Err(Error::Database { .. })), "{failed:?}");
    let reopened = notes_db(place);
    assert_eq!((version(&reopened), tables(&reopened)), (1, vec!["notes".to_owned(), "schema_version".to_owned()]));
}

fn a_newer_database_is_refused(place: &Place) {
    let newer = place.opened(WAIT, &[NOTES, UNHEARD_OF]).expect("newer");
    noted(&newer, "kept");
    drop(newer);
    let before = place.file_path().map(|file| std::fs::read(file).expect("bytes"));

    let refused = place.opened(WAIT, &[NOTES]).expect_err("refused");

    assert!(matches!(refused, Error::Newer { found: 2, known: 1, .. }), "{refused:?}");
    let told = refused.to_string();
    assert!(told.contains("newer version"), "{told}");
    if let Some(file) = place.file_path() {
        assert!(told.contains("app.db"), "{told}");
        assert_eq!(Some(std::fs::read(file).expect("bytes")), before, "not a byte of it changed");
    }
    let newer = place.opened(WAIT, &[NOTES, UNHEARD_OF]).expect("the newer version still opens");
    assert_eq!(notes(&newer), 1);
}

fn a_failed_change_is_rolled_back_whole(place: &Place) {
    let db = notes_db(place);

    let failed = db.write(|conn| {
        conn.execute("INSERT INTO notes (text) VALUES ('half')", &[])?;
        conn.execute("INSERT INTO nowhere (text) VALUES ('other half')", &[])
    });

    assert!(failed.is_err());
    assert_eq!(notes(&db), 0);
}

fn a_failed_read_leaves_the_connection_usable(place: &Place) {
    let db = notes_db(place);

    assert!(db.read(|conn| conn.one::<i64>("SELECT COUNT(*) FROM nowhere", &[])).is_err());

    noted(&db, "after the failed read");
    assert_eq!(notes(&db), 1);
    let kinds = db
        .read(|conn| conn.row("SELECT 1, CAST(2.5 AS DOUBLE PRECISION), 'x', NULL", &[]))
        .expect("row")
        .expect("one row");
    assert_eq!(kinds.0, [Value::Integer(1), Value::Real(2.5), Value::Text("x".into()), Value::Null]);
}

#[test]
fn a_new_database_holds_every_table_at_the_newest_version() {
    holds_every_table_at_the_newest_version(&Place::file());
}

#[test]
fn opening_again_changes_nothing_on_disk() {
    opening_again_changes_nothing(&Place::file());
}

#[test]
fn only_the_migrations_a_database_has_not_seen_are_applied() {
    only_unseen_migrations_are_applied(&Place::file());
}

#[test]
fn a_migration_that_fails_leaves_the_database_at_the_version_it_had() {
    a_failed_migration_leaves_the_version(&Place::file());
}

#[test]
fn a_database_from_a_newer_version_is_refused_and_left_as_it_is() {
    a_newer_database_is_refused(&Place::file());
    assert_eq!(MIGRATIONS.len(), 1, "a second migration is appended to the list, never edited into the first");
}

#[test]
fn a_change_that_fails_is_rolled_back_whole() {
    a_failed_change_is_rolled_back_whole(&Place::file());
}

#[test]
fn a_read_that_fails_leaves_the_connection_usable() {
    a_failed_read_leaves_the_connection_usable(&Place::file());
}

on_postgres! {
    postgres_holds_every_table_at_the_newest_version => holds_every_table_at_the_newest_version,
    postgres_opening_again_changes_nothing => opening_again_changes_nothing,
    postgres_only_unseen_migrations_are_applied => only_unseen_migrations_are_applied,
    postgres_a_failed_migration_leaves_the_version => a_failed_migration_leaves_the_version,
    postgres_a_newer_database_is_refused => a_newer_database_is_refused,
    postgres_a_failed_change_is_rolled_back_whole => a_failed_change_is_rolled_back_whole,
    postgres_a_failed_read_leaves_the_connection_usable => a_failed_read_leaves_the_connection_usable,
    postgres_two_opening_at_once_migrate_once => two_opening_at_once_migrate_once,
}

fn two_opening_at_once_migrate_once(place: &Place) {
    let start = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                start.wait();
                assert_eq!(version(&place.open()), 1);
            });
        }
    });
    assert_eq!(version(&place.open()), 1);
}

#[test]
fn programs_opening_one_database_at_once_migrate_it_once() {
    two_opening_at_once_migrate_once(&Place::file());
}

#[test]
fn a_database_on_disk_is_in_wal_mode_waits_for_writers_and_checks_foreign_keys() {
    let dir = tempfile::tempdir().expect("dir");
    let db = Db::open(&dir.path().join(FILE)).expect("open");

    assert_eq!(pragma(&db, "journal_mode"), "wal");
    assert_eq!(pragma(&db, "synchronous"), "1", "NORMAL");
    assert_eq!(pragma(&db, "foreign_keys"), "1");
    assert_eq!(pragma(&db, "busy_timeout"), "10000");
    for beside in ["app.db", "app.db-wal", "app.db-shm"] {
        assert!(dir.path().join(beside).is_file(), "{beside}");
    }
}

#[cfg(unix)]
#[test]
fn the_database_and_its_journal_are_the_user_s_alone() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("dir");
    let _db = Db::open(&dir.path().join("deeper").join(FILE)).expect("open");

    for beside in ["app.db", "app.db-wal", "app.db-shm"] {
        let mode = std::fs::metadata(dir.path().join("deeper").join(beside)).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{beside}");
    }
}

#[test]
fn a_database_in_memory_has_the_same_tables_and_is_its_own() {
    let (one, other) = (Db::in_memory().expect("one"), Db::in_memory().expect("other"));
    one.write(|conn| conn.execute("INSERT INTO memory_books (project) VALUES ('p')", &[])).expect("write");

    assert_eq!(tables(&one), tables(&other));
    let books = |db: &Db| db.read(|conn| conn.one::<i64>("SELECT COUNT(*) FROM memory_books", &[]));
    assert_eq!((books(&one).expect("one"), books(&other).expect("other")), (Some(1), Some(0)));
}

#[test]
fn a_database_handed_over_is_migrated_like_one_opened_from_a_file() {
    let db = Db::with_database(Database::sqlite_in_memory().expect("database")).expect("migrated");

    assert_eq!(version(&db), 1);
    assert_eq!(tables(&db), tables(&Db::in_memory().expect("in memory")));
}

#[test]
fn a_reader_is_not_stopped_by_a_writer_and_sees_what_was_committed() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join(FILE);
    let (writer, reader) = (Db::opened(&file, WAIT, &[NOTES]).expect("writer"), Db::open_beside(&file, SHORT));
    noted(&writer, "committed");
    let (writing, wrote) = unbounded();
    let (read, done) = unbounded::<()>();

    std::thread::scope(|scope| {
        scope.spawn(move || {
            let held = writer.write(|conn| {
                conn.execute("INSERT INTO notes (text) VALUES ('not yet committed')", &[])?;
                writing.send(()).expect("writing");
                done.recv().expect("done");
                Ok(())
            });
            held.expect("write");
        });
        wrote.recv().expect("wrote");
        assert_eq!(notes(&reader), 1, "read while the other connection is mid-write");
        read.send(()).expect("read");
    });

    assert_eq!(notes(&reader), 2);
}

#[test]
fn a_writer_kept_waiting_past_the_timeout_gets_an_error_that_says_so() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join(FILE);
    let (holder, waiter) = (Db::opened(&file, WAIT, &[NOTES]).expect("holder"), Db::open_beside(&file, SHORT));
    let (holding, held) = unbounded();
    let (gave_up, release) = unbounded::<()>();

    std::thread::scope(|scope| {
        scope.spawn(move || {
            let kept = holder.write(|_| {
                holding.send(()).expect("holding");
                release.recv().expect("release");
                Ok(())
            });
            kept.expect("held");
        });
        held.recv().expect("held");
        let started = Instant::now();
        let refused = waiter.write(|conn| conn.execute("INSERT INTO notes (text) VALUES ('late')", &[]));
        let waited = started.elapsed();
        gave_up.send(()).expect("gave up");

        let refused = refused.expect_err("busy");
        assert!(matches!(refused, Error::Busy { .. }), "{refused:?}");
        assert!(refused.to_string().contains("being written by another process; gave up after 0.15 s"), "{refused}");
        assert!(waited >= SHORT && waited < Duration::from_secs(5), "{waited:?}");
        assert_eq!(std::io::Error::from(refused).kind(), std::io::ErrorKind::TimedOut);
    });

    noted(&waiter, "once the other is done");
    assert_eq!(notes(&waiter), 1);
}

impl Db {
    fn open_beside(file: &Path, wait: Duration) -> Self {
        Self::opened(file, wait, &[NOTES]).expect("beside")
    }
}
