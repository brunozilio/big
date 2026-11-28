use super::*;

#[test]
fn placeholders_become_numbered_for_postgres_outside_quotes() {
    assert_eq!(
        for_postgres("SELECT a FROM t WHERE b = ?1 AND c = ?2 OR d = ?1"),
        "SELECT a FROM t WHERE b = $1 AND c = $2 OR d = $1"
    );
    assert_eq!(for_postgres("INSERT INTO t VALUES (?, ?)"), "INSERT INTO t VALUES ($1, $2)");
    assert_eq!(for_postgres("SELECT '?1' FROM t WHERE a = ?1"), "SELECT '?1' FROM t WHERE a = $1");
}

#[test]
fn the_schema_uses_postgres_types() {
    let made = schema_for_postgres(
        "CREATE TABLE t (id TEXT PRIMARY KEY, n INTEGER NOT NULL, b BLOB, r REAL, integers_seen INTEGER) WITHOUT ROWID;",
    );
    assert_eq!(
        made,
        "CREATE TABLE t (id TEXT PRIMARY KEY, n BIGINT NOT NULL, b BYTEA, r DOUBLE PRECISION, integers_seen BIGINT) ;"
    );
}

fn exercised(db: &Database) {
    let steps = ["CREATE TABLE t (id TEXT PRIMARY KEY, n INTEGER NOT NULL, b BLOB, r REAL)"];
    assert_eq!(db.migrate("schema_steps", &steps).ok(), Some(0));
    assert_eq!(db.migrate("schema_steps", &steps).ok(), Some(1));
    let wrote = db.write(|conn| {
        conn.execute("INSERT INTO t (id, n, b, r) VALUES (?1, ?2, ?3, ?4)", args!["a", 3_i64, vec![1_u8, 2], 1.5_f64])?;
        conn.execute("INSERT INTO t (id, n, b) VALUES (?1, ?2, ?3)", args!["b", 4_i64, Option::<Vec<u8>>::None])
    });
    assert_eq!(wrote.ok(), Some(1));
    let read = db.read(|conn| conn.all::<String>("SELECT id FROM t ORDER BY n DESC", &[]));
    assert_eq!(read.ok(), Some(vec!["b".to_owned(), "a".to_owned()]));
    let row = db.read(|conn| conn.row("SELECT n, b, r FROM t WHERE id = ?1", args!["a"])).ok().flatten();
    let got = row.map(|row| (row.get::<i64>(0).ok(), row.get::<Vec<u8>>(1).ok(), row.get::<f64>(2).ok()));
    assert_eq!(got, Some((Some(3), Some(vec![1, 2]), Some(1.5))));
    let nothing =
        db.read(|conn| conn.one::<Option<Vec<u8>>>("SELECT b FROM t WHERE id = ?1", args!["b"])).ok().flatten();
    assert_eq!(nothing, Some(None));
    let failed = db.write(|conn| {
        conn.execute("INSERT INTO t (id, n) VALUES (?1, ?2)", args!["c", 5_i64])?;
        conn.execute("INSERT INTO t (id, n) VALUES (?1, ?2)", args!["a", 6_i64])
    });
    assert!(failed.as_ref().is_err_and(Error::conflict));
    assert_eq!(db.read(|conn| conn.one::<i64>("SELECT COUNT(*) FROM t", &[])).ok().flatten(), Some(2));
    let upserted = db.write(|conn| {
        conn.execute(
            "INSERT INTO t (id, n) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET n = excluded.n",
            args!["a", 9_i64],
        )?;
        conn.execute("INSERT INTO t (id, n) VALUES (?1, ?2) ON CONFLICT DO NOTHING", args!["a", 10_i64])
    });
    assert_eq!(upserted.ok(), Some(0));
    assert_eq!(db.read(|conn| conn.one::<i64>("SELECT n FROM t WHERE id = ?1", args!["a"])).ok().flatten(), Some(9));
    let flag = db.read(|conn| conn.one::<bool>("SELECT n > ?1 FROM t WHERE id = 'a'", args![1_i64])).ok().flatten();
    assert_eq!(flag, Some(true));
    assert!(db.migrate("schema_steps", &[]).is_err());
}

#[test]
fn sqlite_reads_writes_migrates_and_refuses_a_newer_schema() {
    exercised(&Database::sqlite_in_memory().unwrap_or_else(|_| unreachable!()));
}

#[test]
#[ignore = "needs a PostgreSQL in DATABASE_TEST_URL"]
fn postgres_behaves_like_sqlite() {
    let Ok(url) = std::env::var("DATABASE_TEST_URL") else { return };
    let db = Database::postgres(&Target { url, ca_file: String::new() }).unwrap_or_else(|error| panic!("{error}"));
    db.write(|conn| conn.batch("DROP TABLE IF EXISTS t; DROP TABLE IF EXISTS schema_steps;"))
        .unwrap_or_else(|error| panic!("{error}"));
    exercised(&db);
}

#[test]
#[ignore = "needs a PostgreSQL in DATABASE_TEST_URL"]
fn a_sqlite_database_is_copied_once_into_postgres_parents_first() {
    let Ok(url) = std::env::var("DATABASE_TEST_URL") else { return };
    let target = Database::postgres(&Target { url, ca_file: String::new() }).unwrap_or_else(|error| panic!("{error}"));
    let reset = "DROP TABLE IF EXISTS child; DROP TABLE IF EXISTS parent; DROP TABLE IF EXISTS steps; DROP TABLE IF EXISTS copied_from_sqlite;";
    target.write(|conn| conn.batch(reset)).unwrap_or_else(|error| panic!("{error}"));
    let schema = [
        "CREATE TABLE child (id TEXT PRIMARY KEY, parent TEXT NOT NULL REFERENCES parent (id), n INTEGER) WITHOUT ROWID",
        "CREATE TABLE parent (id TEXT PRIMARY KEY, body BLOB)",
    ];
    let ordered = [schema[1], schema[0]];
    let source = Database::sqlite_in_memory().unwrap_or_else(|_| unreachable!());
    source.migrate("steps", &ordered).unwrap_or_else(|error| panic!("{error}"));
    target.migrate("steps", &ordered).unwrap_or_else(|error| panic!("{error}"));
    source
        .write(|conn| {
            conn.execute("INSERT INTO parent (id, body) VALUES (?1, ?2)", args!["p", vec![7_u8]])?;
            conn.execute("INSERT INTO child (id, parent, n) VALUES (?1, ?2, ?3)", args!["c", "p", 3_i64])
        })
        .unwrap_or_else(|error| panic!("{error}"));
    let first = copy_into(&source, &target, &[]).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(first, Some(Copied { tables: 2, rows: 2 }));
    assert_eq!(copy_into(&source, &target, &[]).ok(), Some(None));
    let n = target.read(|conn| conn.one::<i64>("SELECT n FROM child WHERE parent = ?1", args!["p"])).ok().flatten();
    assert_eq!(n, Some(3));
}

fn handed_over(db: &Database) {
    let old = Lease::new("leader", "old", 10);
    let new = Lease::new("leader", "new", 10);
    assert_eq!(old.take(db, 100).ok(), Some(Turn { held: true, wanted: false }));
    assert_eq!(new.take(db, 101).ok(), Some(Turn { held: false, wanted: false }));
    assert_eq!(old.take(db, 102).ok(), Some(Turn { held: true, wanted: true }));
    assert!(old.release(db).is_ok());
    assert_eq!(new.take(db, 103).ok(), Some(Turn { held: true, wanted: false }));
    assert_eq!(old.take(db, 104).ok(), Some(Turn { held: false, wanted: false }));
    assert_eq!(new.take(db, 105).ok(), Some(Turn { held: true, wanted: true }));
    assert_eq!(old.take(db, 200).ok(), Some(Turn { held: true, wanted: false }));
}

#[test]
fn the_turn_is_handed_over_when_asked_and_taken_when_it_lapses() {
    handed_over(&Database::sqlite_in_memory().unwrap_or_else(|_| unreachable!()));
}

#[test]
#[ignore = "needs a PostgreSQL in DATABASE_TEST_URL"]
fn the_turn_is_handed_over_on_postgres_too() {
    let Ok(url) = std::env::var("DATABASE_TEST_URL") else { return };
    let db = Database::postgres(&Target { url, ca_file: String::new() }).unwrap_or_else(|error| panic!("{error}"));
    db.write(|conn| conn.batch("DROP TABLE IF EXISTS leases")).unwrap_or_else(|error| panic!("{error}"));
    handed_over(&db);
}

#[test]
#[ignore = "needs a PostgreSQL in DATABASE_TEST_URL"]
fn the_password_is_asked_on_every_new_connection() {
    let Ok(url) = std::env::var("DATABASE_TEST_URL") else { return };
    let bare = url.replace(":test@", "@");
    let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = std::sync::Arc::clone(&asked);
    let password: Password = std::sync::Arc::new(move || {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok("test".to_owned())
    });
    let db = Database::postgres_with_password(&Target { url: bare, ca_file: String::new() }, password)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(db.read(|conn| conn.one::<i64>("SELECT 1::BIGINT", &[])).ok().flatten(), Some(1));
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1);
}
