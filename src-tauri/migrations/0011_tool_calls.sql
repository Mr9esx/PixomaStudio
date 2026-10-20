ALTER TABLE runs ADD COLUMN tool_snapshot TEXT CHECK(tool_snapshot IS NULL OR json_valid(tool_snapshot));

CREATE TABLE tool_calls (
    id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    turn INTEGER NOT NULL CHECK(turn > 0),
    part INTEGER NOT NULL CHECK(part >= 0),
    call TEXT NOT NULL CHECK(json_valid(call)),
    definition TEXT CHECK(definition IS NULL OR json_valid(definition)),
    status TEXT NOT NULL DEFAULT 'prepared'
        CHECK(status IN ('prepared','waiting_approval','approved','denied','executing','succeeded','failed','result_unknown')),
    result TEXT CHECK(result IS NULL OR json_valid(result)),
    approved_arguments TEXT CHECK(approved_arguments IS NULL OR json_valid(approved_arguments)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(run_id, id),
    UNIQUE(run_id, turn, part)
);
CREATE INDEX tool_calls_account ON tool_calls(account_id, run_id, turn, part);
