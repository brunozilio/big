use rusqlite::{Connection, TransactionBehavior};

const MIGRATIONS: &[&str] = &[
    "
    CREATE TABLE work_events (
        item TEXT NOT NULL,
        seq INTEGER NOT NULL,
        event TEXT NOT NULL,
        at INTEGER NOT NULL,
        PRIMARY KEY (item, seq)
    ) WITHOUT ROWID;
    CREATE TABLE items (
        id TEXT PRIMARY KEY,
        workflow TEXT NOT NULL,
        parent TEXT,
        version INTEGER NOT NULL,
        ended INTEGER NOT NULL,
        state TEXT NOT NULL
    );
    CREATE TABLE snapshots (
        item TEXT NOT NULL,
        seq INTEGER NOT NULL,
        state TEXT NOT NULL,
        PRIMARY KEY (item, seq)
    ) WITHOUT ROWID;
    CREATE TABLE connections (
        id TEXT PRIMARY KEY,
        provider TEXT NOT NULL,
        label TEXT NOT NULL,
        account TEXT,
        capabilities TEXT NOT NULL,
        settings TEXT NOT NULL
    );
    CREATE TABLE seen_events (
        connection TEXT NOT NULL,
        id TEXT NOT NULL,
        at INTEGER NOT NULL,
        PRIMARY KEY (connection, id)
    ) WITHOUT ROWID;
    CREATE TABLE event_cursors (
        connection TEXT PRIMARY KEY,
        cursor TEXT NOT NULL
    );
    CREATE TABLE branding (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
",
    "
    CREATE TABLE applications (
        id TEXT PRIMARY KEY,
        position INTEGER NOT NULL,
        data TEXT NOT NULL
    );
    CREATE TABLE preferences (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
",
    "
    CREATE TABLE factory_findings (
        id TEXT PRIMARY KEY,
        item TEXT NOT NULL,
        workflow TEXT NOT NULL,
        generation TEXT,
        kind TEXT NOT NULL,
        step TEXT NOT NULL,
        data TEXT NOT NULL,
        at INTEGER NOT NULL
    );
    CREATE INDEX factory_findings_by_step ON factory_findings (workflow, step, kind);
    CREATE TABLE factory_generations (
        version TEXT PRIMARY KEY,
        chain TEXT NOT NULL,
        files TEXT NOT NULL,
        first_seen INTEGER NOT NULL
    );
",
    "
    ALTER TABLE event_cursors ADD COLUMN moved INTEGER NOT NULL DEFAULT 0;
    CREATE INDEX seen_events_by_at ON seen_events (at);
",
    "
    CREATE TABLE archived_items (
        item TEXT PRIMARY KEY,
        at INTEGER NOT NULL
    );
",
    "
    CREATE TABLE connector_memory (
        connection TEXT NOT NULL,
        name TEXT NOT NULL,
        value TEXT NOT NULL,
        PRIMARY KEY (connection, name)
    ) WITHOUT ROWID;
",
];

pub fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch("CREATE TABLE IF NOT EXISTS store_schema (version INTEGER NOT NULL)")?;
    let current: i64 = transaction.query_row("SELECT COUNT(*) FROM store_schema", [], |row| row.get(0))?;
    for (version, migration) in MIGRATIONS.iter().enumerate().skip(usize::try_from(current).unwrap_or(0)) {
        transaction.execute_batch(migration)?;
        transaction.execute(
            "INSERT INTO store_schema (version) VALUES (?1)",
            [i64::try_from(version + 1).unwrap_or(i64::MAX)],
        )?;
    }
    transaction.commit()
}
