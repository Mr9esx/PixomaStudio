ALTER TABLE tool_calls ADD COLUMN raw_result TEXT
    CHECK(raw_result IS NULL OR json_valid(raw_result));
