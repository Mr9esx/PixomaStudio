CREATE UNIQUE INDEX expert_tasks_creation ON expert_tasks(json_extract(document,'$.creation_key'));

CREATE TABLE expert_step_attempts (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    expert_task_id TEXT NOT NULL,
    step_id TEXT NOT NULL,
    creation_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    document TEXT NOT NULL CHECK(json_valid(document)),
    UNIQUE(creation_key),
    FOREIGN KEY(expert_task_id) REFERENCES expert_tasks(id) ON DELETE CASCADE,
    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

ALTER TABLE expert_cards ADD COLUMN tool_call_id TEXT;
CREATE UNIQUE INDEX expert_cards_tool ON expert_cards(tool_call_id);
