CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);
CREATE INDEX messages_session ON messages(account_id, session_id, created_at);

CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    request_id TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    status TEXT GENERATED ALWAYS AS (json_extract(document, '$.status')) STORED,
    checkpoint TEXT,
    checkpoint_version TEXT,
    UNIQUE(account_id, request_id),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);
CREATE INDEX runs_session ON runs(account_id, session_id, created_at);

CREATE TABLE run_events (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    UNIQUE(run_id, sequence),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);

CREATE TABLE flow_nodes (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);

CREATE TABLE flow_edges (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    source TEXT NOT NULL REFERENCES flow_nodes(id) ON DELETE CASCADE,
    target TEXT NOT NULL REFERENCES flow_nodes(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);

CREATE TABLE approvals (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);

CREATE TABLE clarifications (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);

CREATE TABLE workflow_executions (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);

CREATE TABLE expert_tasks (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);

CREATE TABLE expert_cards (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE
);
