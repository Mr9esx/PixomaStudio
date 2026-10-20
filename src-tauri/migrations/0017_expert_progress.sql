CREATE UNIQUE INDEX expert_tasks_identity ON expert_tasks(id,account_id);
CREATE UNIQUE INDEX expert_tasks_creation ON expert_tasks(account_id,json_extract(document,'$.creation_key'));

CREATE TABLE expert_step_attempts (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    expert_task_id TEXT NOT NULL,
    step_id TEXT NOT NULL,
    creation_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    UNIQUE(account_id,creation_key),
    FOREIGN KEY(expert_task_id,account_id) REFERENCES expert_tasks(id,account_id) ON DELETE CASCADE,
    FOREIGN KEY(session_id,account_id) REFERENCES sessions(id,account_id) ON DELETE CASCADE
);

ALTER TABLE expert_cards ADD COLUMN tool_call_id TEXT;
CREATE UNIQUE INDEX expert_cards_tool ON expert_cards(account_id,tool_call_id);
