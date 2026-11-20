ALTER TABLE local_executions ADD COLUMN result_unknown INTEGER NOT NULL DEFAULT 0 CHECK(result_unknown IN (0,1));
