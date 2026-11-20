CREATE TABLE experts (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL CHECK (source IN ('user','official')),
    document TEXT NOT NULL CHECK (json_valid(document))
);

CREATE TABLE expert_versions (
    expert_id TEXT NOT NULL,
    version TEXT NOT NULL,
    document TEXT NOT NULL CHECK (json_valid(document)),
    content_digest TEXT NOT NULL,
    PRIMARY KEY(expert_id,version),
    FOREIGN KEY(expert_id) REFERENCES experts(id)
);

ALTER TABLE sessions ADD COLUMN expert_snapshot TEXT CHECK(expert_snapshot IS NULL OR json_valid(expert_snapshot));
