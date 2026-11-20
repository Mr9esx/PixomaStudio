CREATE TABLE video_materials (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    kind TEXT NOT NULL CHECK(kind IN ('character','scene','style','voice','reference')),
    brief TEXT NOT NULL,
    asset_id TEXT UNIQUE REFERENCES assets(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL
);

ALTER TABLE video_work_chats ADD COLUMN material_id TEXT REFERENCES video_materials(id) ON DELETE RESTRICT;
ALTER TABLE video_work_chats ADD COLUMN selected_version_id TEXT REFERENCES asset_versions(id) ON DELETE RESTRICT;
ALTER TABLE video_work_chats ADD COLUMN source_context TEXT NOT NULL DEFAULT '{}';

CREATE TABLE video_work_materials (
    video_id TEXT NOT NULL REFERENCES video_works(id) ON DELETE CASCADE,
    material_id TEXT NOT NULL REFERENCES video_materials(id) ON DELETE RESTRICT,
    PRIMARY KEY(video_id,material_id)
);

CREATE TABLE video_material_results (
    request_id TEXT PRIMARY KEY,
    request_input TEXT NOT NULL,
    material_id TEXT NOT NULL REFERENCES video_materials(id) ON DELETE RESTRICT,
    source_session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    source_session_title TEXT NOT NULL,
    source_asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    source_version_id TEXT NOT NULL REFERENCES asset_versions(id) ON DELETE RESTRICT,
    asset_version_id TEXT NOT NULL REFERENCES asset_versions(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL
);

INSERT INTO video_materials(id,name,kind,brief,asset_id,created_at)
SELECT 'asset:' || c.asset_id, min(a.name), 'reference', '', c.asset_id, min(s.created_at)
FROM video_work_chats c JOIN assets a ON a.id=c.asset_id JOIN sessions s ON s.id=c.session_id
WHERE c.asset_id IS NOT NULL GROUP BY c.asset_id;

INSERT INTO video_materials(id,name,kind,brief,asset_id,created_at)
SELECT 'chat:' || c.session_id, min(s.title), 'reference', '', NULL, min(s.created_at)
FROM video_work_chats c JOIN sessions s ON s.id=c.session_id
WHERE c.asset_id IS NULL GROUP BY c.session_id;

UPDATE video_work_chats SET material_id=CASE WHEN asset_id IS NULL THEN 'chat:' || session_id ELSE 'asset:' || asset_id END,
selected_version_id=(SELECT v.id FROM asset_versions v JOIN assets a ON a.id=v.asset_id WHERE a.id=video_work_chats.asset_id AND v.version=a.current_version);

INSERT INTO video_work_materials(video_id,material_id) SELECT DISTINCT video_id,material_id FROM video_work_chats;
