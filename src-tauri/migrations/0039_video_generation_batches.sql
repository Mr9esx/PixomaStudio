CREATE TABLE video_generation_batches (
    video_id TEXT NOT NULL REFERENCES video_works(id) ON DELETE RESTRICT,
    id TEXT NOT NULL,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    request TEXT NOT NULL CHECK(json_valid(request)),
    created_at TEXT NOT NULL,
    PRIMARY KEY(video_id,id)
);
CREATE TABLE video_generation_batch_items (
    video_id TEXT NOT NULL,
    batch_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK(position >= 0),
    PRIMARY KEY(video_id,batch_id,generation_id),
    UNIQUE(video_id,batch_id,position),
    FOREIGN KEY(video_id,batch_id) REFERENCES video_generation_batches(video_id,id) ON DELETE RESTRICT,
    FOREIGN KEY(video_id,generation_id) REFERENCES video_generations(video_id,id) ON DELETE RESTRICT
);
