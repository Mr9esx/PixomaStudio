CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX projects_recent ON projects(created_at DESC, id);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
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
    UNIQUE(request_id)
);
CREATE INDEX sessions_recent ON sessions(updated_at DESC, id);
CREATE INDEX sessions_project ON sessions(project_id, updated_at DESC);
