use database::{Database, Dialect};

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
    "
    CREATE TABLE users (
        id TEXT PRIMARY KEY,
        email TEXT NOT NULL UNIQUE,
        name TEXT NOT NULL,
        created INTEGER NOT NULL,
        seen INTEGER NOT NULL
    );
    CREATE TABLE auth_sessions (
        token TEXT PRIMARY KEY,
        \"user\" TEXT NOT NULL,
        created INTEGER NOT NULL,
        expires INTEGER NOT NULL
    );
    CREATE INDEX auth_sessions_by_user ON auth_sessions (\"user\");
    CREATE TABLE auth_codes (
        email TEXT PRIMARY KEY,
        code TEXT NOT NULL,
        expires INTEGER NOT NULL,
        attempts INTEGER NOT NULL
    );
    CREATE TABLE auth_challenges (
        id TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        data TEXT NOT NULL,
        expires INTEGER NOT NULL
    );
    CREATE TABLE passkeys (
        id TEXT PRIMARY KEY,
        \"user\" TEXT NOT NULL,
        public_key BLOB NOT NULL,
        algorithm INTEGER NOT NULL,
        counter INTEGER NOT NULL,
        name TEXT NOT NULL,
        created INTEGER NOT NULL,
        used INTEGER
    );
    CREATE INDEX passkeys_by_user ON passkeys (\"user\");
",
    "
    CREATE TABLE ownership (
        kind TEXT NOT NULL,
        id TEXT NOT NULL,
        owner TEXT NOT NULL,
        PRIMARY KEY (kind, id)
    ) WITHOUT ROWID;
    CREATE INDEX ownership_by_owner ON ownership (kind, owner);
",
    "
    CREATE TABLE git_tokens (
        token TEXT PRIMARY KEY,
        owner TEXT NOT NULL,
        connection TEXT NOT NULL
    );
",
    "
    CREATE INDEX items_by_ended ON items (ended);
",
    "
    CREATE TABLE passwords (
        \"user\" TEXT PRIMARY KEY,
        hash TEXT NOT NULL,
        changed INTEGER NOT NULL
    );
    CREATE TABLE login_failures (
        login TEXT PRIMARY KEY,
        failures INTEGER NOT NULL,
        locked_until INTEGER NOT NULL
    );
",
    "
    CREATE TABLE browser_devices (
        token TEXT PRIMARY KEY,
        device TEXT NOT NULL,
        owner TEXT NOT NULL,
        name TEXT NOT NULL,
        created INTEGER NOT NULL
    );
    CREATE INDEX browser_devices_owner ON browser_devices (owner);
",
    "
    ALTER TABLE connections ADD COLUMN seq INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE applications ADD COLUMN seq INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE factory_findings ADD COLUMN seq INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE items ADD COLUMN seq INTEGER NOT NULL DEFAULT 0;
    CREATE INDEX items_by_seq ON items (seq);
    CREATE INDEX factory_findings_by_seq ON factory_findings (seq);
",
    "
    CREATE TABLE connector_calls (
        at INTEGER NOT NULL,
        connection TEXT NOT NULL,
        action TEXT NOT NULL,
        origin TEXT,
        item TEXT,
        step TEXT,
        ok INTEGER NOT NULL
    );
    CREATE INDEX connector_calls_by_at ON connector_calls (at);
",
    "
    CREATE TABLE spend (
        at INTEGER NOT NULL,
        item TEXT NOT NULL,
        step TEXT NOT NULL,
        agent TEXT NOT NULL,
        application TEXT NOT NULL,
        owner TEXT NOT NULL,
        micros INTEGER NOT NULL,
        unpriced INTEGER NOT NULL
    );
    CREATE INDEX spend_by_at ON spend (at);
",
];

const ORDERED: [&str; 4] = ["connections", "applications", "factory_findings", "items"];

pub fn migrate(database: &Database) -> database::Result<()> {
    database.migrate("store_schema", MIGRATIONS)?;
    if database.dialect() != Dialect::Sqlite {
        return Ok(());
    }
    database.write(|conn| {
        for table in ORDERED {
            conn.execute(&format!("UPDATE {table} SET seq = rowid WHERE seq = 0"), &[])?;
        }
        Ok(())
    })
}

pub fn next_seq(table: &str) -> String {
    format!("(SELECT COALESCE(MAX(seq), 0) + 1 FROM {table})")
}
