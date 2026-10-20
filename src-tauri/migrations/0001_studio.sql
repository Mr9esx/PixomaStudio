CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL CHECK (length(trim(account_id)) > 0),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX projects_account ON projects(account_id, created_at DESC);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL CHECK (length(trim(account_id)) > 0),
    request_id TEXT NOT NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    permission_mode TEXT NOT NULL DEFAULT 'request_approval'
        CHECK(permission_mode IN ('request_approval','auto_approve','full_access')),
    model_config_id TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status = 'active'),
    expert_id TEXT,
    expert_version TEXT,
    expert_name TEXT,
    current_expert_task_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(account_id, request_id)
);
CREATE INDEX sessions_account_recent ON sessions(account_id, updated_at DESC, id);
CREATE INDEX sessions_project ON sessions(account_id, project_id, updated_at DESC);
