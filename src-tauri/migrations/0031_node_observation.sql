ALTER TABLE compute_nodes ADD COLUMN capabilities TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(capabilities));
ALTER TABLE compute_nodes ADD COLUMN started_at TEXT;
ALTER TABLE compute_nodes ADD COLUMN comfy_version TEXT NOT NULL DEFAULT '';
CREATE TABLE compute_node_metrics (
    node_id TEXT NOT NULL REFERENCES compute_nodes(id) ON DELETE CASCADE,
    received_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    PRIMARY KEY(node_id,received_at)
);
