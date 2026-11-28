const SESSIONS_AND_MEMORY: &str = "
CREATE TABLE sessions (
    session TEXT PRIMARY KEY NOT NULL,
    cwd TEXT NOT NULL,
    title TEXT,
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL,
    status TEXT NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0,
    unread INTEGER NOT NULL DEFAULT 0,
    tie INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX sessions_newest_first ON sessions (updated DESC, tie);
CREATE INDEX sessions_by_cwd ON sessions (cwd, updated DESC, tie);

CREATE TABLE memory_books (
    project TEXT PRIMARY KEY NOT NULL,
    next INTEGER NOT NULL DEFAULT 0,
    revision INTEGER NOT NULL DEFAULT 0,
    with_runs INTEGER NOT NULL DEFAULT 0,
    with_corrections INTEGER NOT NULL DEFAULT 0,
    with_cost_usd REAL NOT NULL DEFAULT 0,
    without_runs INTEGER NOT NULL DEFAULT 0,
    without_corrections INTEGER NOT NULL DEFAULT 0,
    without_cost_usd REAL NOT NULL DEFAULT 0,
    briefed INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE memories (
    project TEXT NOT NULL REFERENCES memory_books (project) ON DELETE CASCADE,
    id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    forgotten_at INTEGER,
    kind TEXT NOT NULL,
    author TEXT NOT NULL,
    text TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    touches TEXT NOT NULL DEFAULT '[]',
    history TEXT NOT NULL DEFAULT '[]',
    created INTEGER NOT NULL,
    origin TEXT,
    uses INTEGER NOT NULL DEFAULT 0,
    last_used INTEGER NOT NULL DEFAULT 0,
    credit TEXT,
    PRIMARY KEY (project, id)
);

CREATE TABLE memory_undo_points (
    project TEXT PRIMARY KEY NOT NULL,
    book TEXT NOT NULL
);
";

const MEMORY_LINEAGE: &str = "ALTER TABLE memories ADD COLUMN lineage TEXT";

pub(super) const MIGRATIONS: &[&str] = &[SESSIONS_AND_MEMORY, MEMORY_LINEAGE];
