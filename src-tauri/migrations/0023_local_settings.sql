ALTER TABLE model_configs ADD COLUMN api_key TEXT NOT NULL DEFAULT '';
ALTER TABLE model_configs DROP COLUMN credential_ref;
ALTER TABLE connectors ADD COLUMN credential TEXT NOT NULL DEFAULT '';
ALTER TABLE connectors DROP COLUMN credential_ref;
