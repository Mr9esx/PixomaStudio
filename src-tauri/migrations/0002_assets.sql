CREATE TABLE blobs (
    digest TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    created_at TEXT NOT NULL
);

CREATE TABLE assets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    kind TEXT NOT NULL,
    origin TEXT NOT NULL,
    current_version INTEGER NOT NULL CHECK(current_version > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX assets_recent ON assets(updated_at DESC,id);

CREATE TABLE asset_versions (
    id TEXT PRIMARY KEY,
    asset_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version > 0),
    digest TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    format TEXT NOT NULL,
    width_px INTEGER,
    height_px INTEGER,
    metadata TEXT NOT NULL DEFAULT '{}',
    content_origin TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(asset_id, version),
    UNIQUE(id, asset_id),
    FOREIGN KEY(asset_id) REFERENCES assets(id) ON DELETE CASCADE,
    FOREIGN KEY(digest) REFERENCES blobs(digest)
);

CREATE TABLE asset_usages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    asset_version_id TEXT NOT NULL,
    session_title_snapshot TEXT NOT NULL,
    usage_kind TEXT NOT NULL,
    run_id TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(session_id, asset_id, asset_version_id, usage_kind),
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY(asset_id) REFERENCES assets(id),
    FOREIGN KEY(asset_version_id, asset_id) REFERENCES asset_versions(id, asset_id)
);

CREATE TABLE asset_operations (
    request_id TEXT PRIMARY KEY,
    operation TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE
);
