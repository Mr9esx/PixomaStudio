CREATE TABLE categories (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX categories_project ON categories(project_id, parent_id);

CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE CHECK(length(trim(name)) > 0)
);

CREATE TABLE project_assets (
    id TEXT PRIMARY KEY,
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
    FOREIGN KEY(asset_id) REFERENCES assets(id),
    FOREIGN KEY(asset_version_id, asset_id) REFERENCES asset_versions(id, asset_id)
);
CREATE UNIQUE INDEX project_assets_placement ON project_assets(coalesce(project_id,''), asset_id);
CREATE INDEX project_assets_project ON project_assets(project_id, added_at);

CREATE TABLE project_asset_tags (
    project_asset_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY(project_asset_id, tag_id),
    FOREIGN KEY(project_asset_id) REFERENCES project_assets(id) ON DELETE CASCADE,
    FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE preferences (
    id INTEGER PRIMARY KEY CHECK(id=1),
    document TEXT NOT NULL CHECK(json_valid(document))
);
