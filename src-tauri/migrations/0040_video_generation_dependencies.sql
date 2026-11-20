ALTER TABLE video_generations ADD COLUMN dependency_state TEXT NOT NULL DEFAULT 'ready'
    CHECK(dependency_state IN ('ready','waiting','waiting_output','blocked','cancelled'));
ALTER TABLE video_generations ADD COLUMN dependencies TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(dependencies));
ALTER TABLE video_generations ADD COLUMN dependency_error TEXT;
