ALTER TABLE runs ADD COLUMN context_snapshot TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(context_snapshot));
