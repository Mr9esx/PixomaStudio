CREATE TABLE local_executions (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,
    request_digest TEXT NOT NULL,
    workflow_id TEXT NOT NULL REFERENCES workflows(id),
    workflow_version_id TEXT,
    node_id TEXT NOT NULL REFERENCES compute_nodes(id),
    session_id TEXT REFERENCES sessions(id),
    definition TEXT NOT NULL CHECK(json_valid(definition)),
    package TEXT NOT NULL CHECK(json_valid(package)),
    content_digest TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending','dispatching','running','collecting','succeeded','failed','cancelled','submission_unknown')),
    attempt_id TEXT UNIQUE,
    prompt_id TEXT,
    sequence INTEGER NOT NULL DEFAULT 0 CHECK(sequence >= 0),
    cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK(cancel_requested IN (0,1)),
    waiting INTEGER NOT NULL DEFAULT 1 CHECK(waiting IN (0,1)),
    heartbeat_at TEXT,
    error TEXT NOT NULL DEFAULT '',
    completion TEXT CHECK(completion IS NULL OR json_valid(completion)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(workflow_version_id,workflow_id) REFERENCES workflow_versions(id,workflow_id)
);
CREATE INDEX local_executions_node ON local_executions(node_id,status,created_at,id);
CREATE UNIQUE INDEX local_executions_one_active_node ON local_executions(node_id)
    WHERE attempt_id IS NOT NULL AND status NOT IN ('succeeded','failed','cancelled');
CREATE TABLE local_execution_events (
    execution_id TEXT NOT NULL REFERENCES local_executions(id),
    attempt_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    document TEXT NOT NULL CHECK(json_valid(document)),
    created_at TEXT NOT NULL,
    PRIMARY KEY(execution_id,sequence)
);
CREATE TABLE local_execution_outputs (
    execution_id TEXT NOT NULL REFERENCES local_executions(id),
    output_id TEXT NOT NULL,
    field_key TEXT NOT NULL,
    output_index INTEGER NOT NULL CHECK(output_index >= 0),
    sha256 TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes > 0),
    media_type TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    asset_version_id TEXT NOT NULL,
    PRIMARY KEY(execution_id,output_id),
    UNIQUE(execution_id,field_key,output_index),
    FOREIGN KEY(asset_version_id,asset_id) REFERENCES asset_versions(id,asset_id)
);
