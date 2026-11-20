ALTER TABLE online_executions ADD COLUMN provider_response TEXT CHECK(provider_response IS NULL OR json_valid(provider_response));
