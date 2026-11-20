CREATE TABLE context_compactions (
    session_id TEXT NOT NULL,
    source_key TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(session_id, source_key),
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
