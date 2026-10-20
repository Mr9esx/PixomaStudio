CREATE TABLE asset_syncs (
    account_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('skill', 'expert')),
    asset_id TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    PRIMARY KEY(account_id, kind, asset_id)
);

CREATE TABLE asset_publication_requests (
    account_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    PRIMARY KEY(account_id, request_id)
);
