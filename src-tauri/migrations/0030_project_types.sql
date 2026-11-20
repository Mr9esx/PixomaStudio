ALTER TABLE projects ADD COLUMN project_type TEXT NOT NULL DEFAULT 'general'
    CHECK (project_type IN ('general', 'video'));
