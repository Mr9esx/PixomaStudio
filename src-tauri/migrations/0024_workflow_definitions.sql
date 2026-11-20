CREATE TABLE workflows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    type TEXT NOT NULL CHECK(type IN ('comfyui','online')),
    category TEXT NOT NULL CHECK(length(trim(category)) > 0),
    tags TEXT NOT NULL CHECK(json_valid(tags)),
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    draft TEXT NOT NULL CHECK(json_valid(draft)),
    default_version_id TEXT,
    revision INTEGER NOT NULL CHECK(revision > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(default_version_id,id) REFERENCES workflow_versions(id,workflow_id)
        DEFERRABLE INITIALLY DEFERRED
);
CREATE INDEX workflows_recent ON workflows(updated_at DESC,id);

CREATE TABLE workflow_versions (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES workflows(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    definition TEXT NOT NULL CHECK(json_valid(definition)),
    content_digest TEXT NOT NULL,
    notes TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(workflow_id,version),
    UNIQUE(id,workflow_id)
);
CREATE INDEX workflow_versions_recent ON workflow_versions(workflow_id,created_at DESC,id);
CREATE TRIGGER workflow_versions_immutable
BEFORE UPDATE ON workflow_versions
BEGIN
    SELECT RAISE(ABORT,'工作流版本内容只能读取');
END;
