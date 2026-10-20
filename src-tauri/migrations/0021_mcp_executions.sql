CREATE TABLE tool_calls_updated (
    id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    turn INTEGER NOT NULL CHECK(turn > 0),
    part INTEGER NOT NULL CHECK(part >= 0),
    call TEXT NOT NULL CHECK(json_valid(call)),
    definition TEXT CHECK(definition IS NULL OR json_valid(definition)),
    status TEXT NOT NULL DEFAULT 'prepared'
        CHECK(status IN ('prepared','waiting_approval','approved','denied','executing','waiting_workflow','waiting_mcp','succeeded','failed','result_unknown','not_executed')),
    result TEXT CHECK(result IS NULL OR json_valid(result)),
    raw_result TEXT CHECK(raw_result IS NULL OR json_valid(raw_result)),
    approved_arguments TEXT CHECK(approved_arguments IS NULL OR json_valid(approved_arguments)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    ended_at TEXT,
    PRIMARY KEY(run_id, id),
    UNIQUE(run_id, turn, part)
);

INSERT INTO tool_calls_updated
    (id,account_id,run_id,turn,part,call,definition,status,result,raw_result,approved_arguments,created_at,updated_at,started_at,ended_at)
SELECT id,account_id,run_id,turn,part,call,definition,status,result,raw_result,approved_arguments,created_at,updated_at,started_at,ended_at
FROM tool_calls;

DROP TABLE tool_calls;
ALTER TABLE tool_calls_updated RENAME TO tool_calls;
CREATE INDEX tool_calls_account ON tool_calls(account_id,run_id,turn,part);

CREATE TABLE mcp_executions (
    id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    document TEXT NOT NULL CHECK(json_valid(document)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(run_id,id)
);
CREATE INDEX mcp_executions_account ON mcp_executions(account_id,run_id,id);
