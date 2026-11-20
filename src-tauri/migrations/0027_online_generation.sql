CREATE TABLE generation_connections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL CHECK(provider='seedance'),
    api_key TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE online_executions (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,
    request_digest TEXT NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(id),
    workflow_version_id TEXT,
    session_id TEXT REFERENCES sessions(id),
    definition TEXT NOT NULL CHECK(json_valid(definition)),
    inputs TEXT NOT NULL CHECK(json_valid(inputs)),
    request_body TEXT NOT NULL CHECK(json_valid(request_body)),
    connection_id TEXT NOT NULL REFERENCES generation_connections(id),
    provider_task_id TEXT,
    status TEXT NOT NULL,
    error TEXT NOT NULL DEFAULT '',
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    outputs TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(outputs)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
