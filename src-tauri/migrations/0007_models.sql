CREATE TABLE model_configs (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    credential_ref TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(id,account_id)
);
CREATE UNIQUE INDEX model_configs_default ON model_configs(account_id) WHERE json_extract(document,'$.default')=1;
