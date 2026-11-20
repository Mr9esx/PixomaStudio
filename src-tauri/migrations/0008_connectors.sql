CREATE TABLE connectors (
    id TEXT PRIMARY KEY,
    document TEXT NOT NULL CHECK(json_valid(document)),
    credential_ref TEXT,
    tools TEXT CHECK(tools IS NULL OR json_valid(tools)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
