CREATE TABLE video_work_exports (
    video_id TEXT NOT NULL REFERENCES video_works(id) ON DELETE RESTRICT,
    request_id TEXT NOT NULL UNIQUE,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    request TEXT NOT NULL CHECK(json_valid(request)),
    project TEXT NOT NULL CHECK(json_valid(project)),
    source_version INTEGER,
    status TEXT NOT NULL CHECK(status IN ('exporting','completed','failed')),
    error TEXT,
    asset_id TEXT REFERENCES assets(id) ON DELETE RESTRICT,
    asset_version_id TEXT REFERENCES asset_versions(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(video_id,request_id)
);
