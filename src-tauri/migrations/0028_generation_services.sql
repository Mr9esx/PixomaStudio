CREATE TABLE generation_services (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    base_url TEXT NOT NULL,
    api_key TEXT NOT NULL,
    created_at TEXT NOT NULL
);
INSERT INTO generation_services SELECT id,name,provider,'https://ark.cn-beijing.volces.com',api_key,created_at FROM generation_connections;
CREATE TABLE online_executions_v2 (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,
    request_digest TEXT NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(id),
    workflow_version_id TEXT,
    session_id TEXT REFERENCES sessions(id),
    definition TEXT NOT NULL CHECK(json_valid(definition)),
    inputs TEXT NOT NULL CHECK(json_valid(inputs)),
    request_body TEXT NOT NULL CHECK(json_valid(request_body)),
    connection_id TEXT NOT NULL REFERENCES generation_services(id),
    provider_task_id TEXT,
    status TEXT NOT NULL,
    error TEXT NOT NULL DEFAULT '',
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    outputs TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(outputs)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
INSERT INTO online_executions_v2 SELECT * FROM online_executions;
DROP TABLE online_executions;
ALTER TABLE online_executions_v2 RENAME TO online_executions;
DROP TABLE generation_connections;
