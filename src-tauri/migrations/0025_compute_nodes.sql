CREATE TABLE compute_nodes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0),
    description TEXT NOT NULL,
    location TEXT NOT NULL CHECK(location IN ('local','remote')),
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    comfyui_base_url TEXT NOT NULL,
    credential_hash TEXT NOT NULL UNIQUE,
    configuration_revision INTEGER NOT NULL CHECK(configuration_revision > 0),
    agent_version TEXT,
    agent_instance_id TEXT,
    started_id TEXT,
    last_seen_at TEXT,
    reported_revision INTEGER,
    comfyui_ready INTEGER NOT NULL DEFAULT 0 CHECK(comfyui_ready IN (0,1)),
    comfyui_error TEXT NOT NULL DEFAULT '',
    hardware TEXT CHECK(hardware IS NULL OR json_valid(hardware)),
    metrics TEXT CHECK(metrics IS NULL OR json_valid(metrics)),
    object_info TEXT CHECK(object_info IS NULL OR json_valid(object_info)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX compute_nodes_recent ON compute_nodes(created_at,id);
