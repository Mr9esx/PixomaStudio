ALTER TABLE runs ADD COLUMN request_digest TEXT NOT NULL DEFAULT '';
ALTER TABLE runs ADD COLUMN model_snapshot TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(model_snapshot));
CREATE UNIQUE INDEX one_running_run_per_session ON runs(session_id) WHERE status IN ('running','waiting_approval','waiting_clarification','waiting_workflow');
