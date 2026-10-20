CREATE TABLE context_compactions (
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    source_key TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(account_id, session_id, source_key),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);
