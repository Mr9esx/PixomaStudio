CREATE TABLE experts (
    account_id TEXT NOT NULL,
    id TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('user','official')),
    document TEXT NOT NULL CHECK (json_valid(document)),
    PRIMARY KEY(account_id,id)
);

CREATE TABLE expert_versions (
    account_id TEXT NOT NULL,
    expert_id TEXT NOT NULL,
    version TEXT NOT NULL,
    document TEXT NOT NULL CHECK (json_valid(document)),
    content_digest TEXT NOT NULL,
    PRIMARY KEY(account_id,expert_id,version),
    FOREIGN KEY(account_id,expert_id) REFERENCES experts(account_id,id)
);

ALTER TABLE sessions ADD COLUMN expert_snapshot TEXT CHECK(expert_snapshot IS NULL OR json_valid(expert_snapshot));
