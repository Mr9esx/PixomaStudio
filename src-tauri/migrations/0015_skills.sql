CREATE TABLE skills (
    account_id TEXT NOT NULL,
    id TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('user','official')),
    document TEXT NOT NULL CHECK (json_valid(document)),
    PRIMARY KEY(account_id,id)
);

CREATE TABLE skill_versions (
    account_id TEXT NOT NULL,
    skill_id TEXT NOT NULL,
    version TEXT NOT NULL,
    document TEXT NOT NULL CHECK (json_valid(document)),
    content_digest TEXT NOT NULL,
    PRIMARY KEY(account_id,skill_id,version),
    FOREIGN KEY(account_id,skill_id) REFERENCES skills(account_id,id)
);
