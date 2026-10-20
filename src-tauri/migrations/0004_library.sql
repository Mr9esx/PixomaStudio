CREATE TABLE categories (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX categories_project ON categories(account_id, project_id, parent_id);

CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    UNIQUE(account_id, name),
    UNIQUE(id, account_id)
);

CREATE TABLE project_assets (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    asset_id TEXT NOT NULL,
    asset_version_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    rating INTEGER NOT NULL DEFAULT 0 CHECK(rating BETWEEN 0 AND 5),
    added_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    archived_at TEXT,
    copy_source TEXT CHECK(copy_source IS NULL OR json_valid(copy_source)),
    UNIQUE(id, account_id),
    FOREIGN KEY(asset_id, account_id) REFERENCES assets(id, account_id),
    FOREIGN KEY(asset_version_id, account_id) REFERENCES asset_versions(id, account_id)
);
CREATE UNIQUE INDEX project_assets_placement ON project_assets(account_id, coalesce(project_id,''), asset_id);
CREATE INDEX project_assets_project ON project_assets(account_id, project_id, added_at);

CREATE TABLE project_asset_tags (
    account_id TEXT NOT NULL,
    project_asset_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY(project_asset_id, tag_id),
    FOREIGN KEY(project_asset_id, account_id) REFERENCES project_assets(id, account_id) ON DELETE CASCADE,
    FOREIGN KEY(tag_id, account_id) REFERENCES tags(id, account_id) ON DELETE CASCADE
);

CREATE TABLE preferences (
    account_id TEXT PRIMARY KEY,
    document TEXT NOT NULL CHECK(json_valid(document))
);
