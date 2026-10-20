CREATE TABLE model_requests (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    request TEXT NOT NULL CHECK(json_valid(request)),
    response TEXT CHECK(response IS NULL OR json_valid(response)),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);
CREATE INDEX model_requests_session ON model_requests(account_id, session_id, created_at DESC, id);

CREATE TABLE context_events (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);
CREATE INDEX context_events_session ON context_events(account_id, session_id, created_at DESC, id);

ALTER TABLE tool_calls ADD COLUMN started_at TEXT;
ALTER TABLE tool_calls ADD COLUMN ended_at TEXT;
