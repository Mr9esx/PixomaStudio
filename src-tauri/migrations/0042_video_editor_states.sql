CREATE TABLE video_editor_states (
    video_id TEXT PRIMARY KEY REFERENCES video_works(id) ON DELETE CASCADE,
    document TEXT NOT NULL CHECK(json_valid(document) AND json_type(document) = 'object'),
    updated_at TEXT NOT NULL
);
