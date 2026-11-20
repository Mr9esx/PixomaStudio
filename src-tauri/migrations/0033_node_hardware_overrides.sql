ALTER TABLE compute_nodes ADD COLUMN hardware_override TEXT CHECK(hardware_override IS NULL OR json_valid(hardware_override));
