CREATE UNIQUE INDEX category_names ON categories(coalesce(project_id,''),coalesce(parent_id,''),name);
