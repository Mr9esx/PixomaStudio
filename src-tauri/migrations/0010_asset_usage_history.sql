CREATE TABLE asset_usages_retained (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    asset_version_id TEXT NOT NULL,
    session_title_snapshot TEXT NOT NULL,
    usage_kind TEXT NOT NULL,
    run_id TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(session_id, asset_id, asset_version_id, usage_kind),
    FOREIGN KEY(asset_id) REFERENCES assets(id),
    FOREIGN KEY(asset_version_id, asset_id) REFERENCES asset_versions(id, asset_id)
);

INSERT INTO asset_usages_retained
SELECT id, session_id, asset_id, asset_version_id,
       session_title_snapshot, usage_kind, run_id, created_at
FROM asset_usages;

DROP TABLE asset_usages;
ALTER TABLE asset_usages_retained RENAME TO asset_usages;

CREATE TRIGGER asset_usages_require_session
BEFORE INSERT ON asset_usages
WHEN NOT EXISTS (
    SELECT 1 FROM sessions
    WHERE id = NEW.session_id
)
BEGIN
    SELECT RAISE(ABORT, '资产引用需要有效会话');
END;
