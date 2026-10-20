CREATE TABLE workflow_catalog (
    account_id TEXT NOT NULL,
    id TEXT NOT NULL,
    version_id INTEGER NOT NULL CHECK(version_id > 0),
    PRIMARY KEY(account_id,id)
);

CREATE TABLE workflow_versions (
    account_id TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    version_id INTEGER NOT NULL CHECK(version_id > 0),
    content_digest TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    PRIMARY KEY(account_id,workflow_id,version_id),
    FOREIGN KEY(account_id,workflow_id) REFERENCES workflow_catalog(account_id,id)
);
