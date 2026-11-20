CREATE TABLE skills (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL CHECK (source IN ('user','official')),
    document TEXT NOT NULL CHECK (json_valid(document))
);

CREATE TABLE skill_versions (
    skill_id TEXT NOT NULL,
    version TEXT NOT NULL,
    document TEXT NOT NULL CHECK (json_valid(document)),
    content_digest TEXT NOT NULL,
    PRIMARY KEY(skill_id,version),
    FOREIGN KEY(skill_id) REFERENCES skills(id)
);
