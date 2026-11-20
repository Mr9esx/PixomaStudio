CREATE TABLE model_configs (
    id TEXT PRIMARY KEY,
    document TEXT NOT NULL CHECK(json_valid(document)),
    credential_ref TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX model_configs_default ON model_configs((1)) WHERE json_extract(document,'$.default')=1;
