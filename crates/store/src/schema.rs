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
        origin TEXT NOT NULL,
        micros INTEGER NOT NULL,
        unpriced INTEGER NOT NULL
    );
    CREATE INDEX spend_by_agent ON spend (agent, at);
    CREATE INDEX spend_by_application ON spend (application, at);
    CREATE INDEX spend_by_owner ON spend (owner, at);
    CREATE INDEX spend_by_at ON spend (at);
",
    "
    CREATE TABLE audit_trail (
        seq INTEGER PRIMARY KEY,
        at INTEGER NOT NULL,
        actor TEXT,
        subject TEXT,
        action TEXT NOT NULL,
        policy TEXT,
        outcome TEXT,
        detail TEXT NOT NULL,
        prev TEXT NOT NULL,
        hash TEXT NOT NULL
    );
    CREATE INDEX audit_trail_by_at ON audit_trail (at);
    CREATE INDEX audit_trail_by_actor ON audit_trail (actor, at);
    CREATE INDEX audit_trail_by_subject ON audit_trail (subject, at);
    CREATE INDEX audit_trail_by_action ON audit_trail (action, at);
",
    "
    ALTER TABLE audit_trail ADD COLUMN schema_version INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE audit_trail ADD COLUMN operation TEXT;
    ALTER TABLE audit_trail ADD COLUMN friendly_name TEXT;
    ALTER TABLE audit_trail ADD COLUMN policy_version TEXT;
    ALTER TABLE audit_trail ADD COLUMN decision_code TEXT;
    ALTER TABLE audit_trail ADD COLUMN layer TEXT;
    ALTER TABLE audit_trail ADD COLUMN failure_stage TEXT;
    ALTER TABLE audit_trail ADD COLUMN approval_wait_ms INTEGER;
    ALTER TABLE audit_trail ADD COLUMN authorized_by TEXT;
    ALTER TABLE audit_trail ADD COLUMN creator_type TEXT;
    ALTER TABLE audit_trail ADD COLUMN agent TEXT;
    CREATE INDEX audit_trail_by_operation ON audit_trail (operation, at);
",
    "
    CREATE TABLE user_roles (
        \"user\" TEXT NOT NULL,
        source TEXT NOT NULL,
        role TEXT NOT NULL,
        PRIMARY KEY (\"user\", source, role)
    );
",
    "
    CREATE TABLE audit_delivery (
        sink TEXT PRIMARY KEY,
        delivered INTEGER NOT NULL,
        dropped INTEGER NOT NULL,
        attempts INTEGER NOT NULL,
        failure TEXT,
        at INTEGER NOT NULL
    );
",
];

const SQLITE_GUARD: &str = "CREATE TRIGGER IF NOT EXISTS audit_trail_only_grows BEFORE UPDATE ON audit_trail
    BEGIN SELECT RAISE(ABORT, 'the audit trail only grows'); END;";

const GUARDING: i64 = 0x7374_6f72_655f_6175;

const POSTGRES_GUARD: &str = "DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'audit_trail_only_grows'
        AND pronamespace = current_schema()::regnamespace) THEN
        CREATE FUNCTION audit_trail_only_grows() RETURNS trigger LANGUAGE plpgsql
        AS $f$ BEGIN RAISE EXCEPTION 'the audit trail only grows'; END $f$;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'audit_trail_only_grows'
        AND tgrelid = 'audit_trail'::regclass) THEN
        CREATE TRIGGER audit_trail_only_grows BEFORE UPDATE ON audit_trail
        FOR EACH ROW EXECUTE FUNCTION audit_trail_only_grows();
    END IF;
END $$;";

fn guarded_on_postgres(conn: &mut dyn database::Conn) -> database::Result<()> {
    conn.batch(&format!("SELECT pg_advisory_xact_lock({GUARDING})"))?;
    conn.batch(POSTGRES_GUARD)
}

const ORDERED: [&str; 4] = ["connections", "applications", "factory_findings", "items"];

pub fn migrate(database: &Database) -> database::Result<()> {
    database.migrate("store_schema", MIGRATIONS)?;
    if database.dialect() != Dialect::Sqlite {
        return database.write(guarded_on_postgres);
    }
    database.write(|conn| conn.batch(SQLITE_GUARD))?;
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
