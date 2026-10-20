CREATE UNIQUE INDEX sessions_identity ON sessions(id, account_id);

CREATE TABLE blobs (
    account_id TEXT NOT NULL,
    digest TEXT NOT NULL,
    path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
    created_at TEXT NOT NULL,
    PRIMARY KEY(account_id, digest)
);

CREATE TABLE assets (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    kind TEXT NOT NULL,
    origin TEXT NOT NULL,
    current_version INTEGER NOT NULL CHECK(current_version > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(id, account_id)
);
CREATE INDEX assets_account ON assets(account_id, updated_at DESC);

CREATE TABLE asset_versions (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
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
    UNIQUE(id, account_id),
    FOREIGN KEY(asset_id, account_id) REFERENCES assets(id, account_id) ON DELETE CASCADE,
    FOREIGN KEY(account_id, digest) REFERENCES blobs(account_id, digest)
);

CREATE TABLE asset_usages (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    asset_version_id TEXT NOT NULL,
    session_title_snapshot TEXT NOT NULL,
    usage_kind TEXT NOT NULL,
    run_id TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(session_id, asset_id, asset_version_id, usage_kind),
    FOREIGN KEY(session_id, account_id) REFERENCES sessions(id, account_id) ON DELETE CASCADE,
    FOREIGN KEY(asset_id, account_id) REFERENCES assets(id, account_id),
    FOREIGN KEY(asset_version_id, account_id) REFERENCES asset_versions(id, account_id)
);

CREATE TABLE asset_operations (
    account_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    PRIMARY KEY(account_id, request_id)
);
