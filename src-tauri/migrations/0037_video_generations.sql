CREATE TABLE video_generations (
    video_id TEXT NOT NULL REFERENCES video_works(id) ON DELETE RESTRICT,
    id TEXT NOT NULL,
    shot_id TEXT NOT NULL,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    execution_request_id TEXT NOT NULL UNIQUE,
    execution_id TEXT,
    snapshot TEXT NOT NULL CHECK(json_valid(snapshot)),
    retry_of TEXT,
    submission_error TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY(video_id,id),
    FOREIGN KEY(video_id,retry_of) REFERENCES video_generations(video_id,id)
);
CREATE INDEX video_generations_shot ON video_generations(video_id,shot_id,created_at);
