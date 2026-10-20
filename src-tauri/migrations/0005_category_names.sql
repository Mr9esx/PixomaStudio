CREATE UNIQUE INDEX category_names ON categories(account_id,coalesce(project_id,''),coalesce(parent_id,''),name);
