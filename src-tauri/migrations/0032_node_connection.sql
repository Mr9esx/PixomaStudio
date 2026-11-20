ALTER TABLE compute_nodes ADD COLUMN agent_token TEXT NOT NULL DEFAULT '';
ALTER TABLE compute_nodes ADD COLUMN local_agent_enabled INTEGER NOT NULL DEFAULT 1 CHECK(local_agent_enabled IN (0,1));
ALTER TABLE compute_nodes ADD COLUMN local_agent_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE compute_nodes ADD COLUMN local_agent_error TEXT NOT NULL DEFAULT '';
CREATE TABLE compute_connection (
    id INTEGER PRIMARY KEY CHECK(id=1),
    listen_address TEXT NOT NULL,
    public_url TEXT NOT NULL
);
INSERT INTO compute_connection(id,listen_address,public_url) VALUES(1,'127.0.0.1:18083','');
