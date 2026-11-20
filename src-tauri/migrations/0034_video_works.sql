CREATE TABLE video_works (
    id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE RESTRICT,
    main_session_id TEXT UNIQUE REFERENCES sessions(id) ON DELETE SET NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL
);

CREATE TABLE video_work_chats (
    video_id TEXT NOT NULL REFERENCES video_works(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    asset_id TEXT REFERENCES assets(id) ON DELETE RESTRICT,
    creation_input TEXT,
    PRIMARY KEY (video_id, session_id)
);

CREATE INDEX video_works_project ON video_works(project_id);
CREATE INDEX video_work_chats_session ON video_work_chats(session_id);

INSERT INTO video_works(id, main_session_id, project_id)
SELECT operations.asset_id, sessions.id, sessions.project_id
FROM asset_operations AS operations
JOIN sessions ON operations.request_id = 'video-project:init:' || sessions.id
WHERE operations.operation = 'create';
