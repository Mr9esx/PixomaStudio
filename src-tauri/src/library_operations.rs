use crate::{Error, Result, Store, error::timestamp, local_api::text};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LibraryBatch {
    project_asset_ids: Vec<String>,
    action: String,
    category_id: Option<String>,
    tag_ids: Option<Vec<String>>,
    rating: Option<i64>,
    archived: Option<bool>,
    project_id: Option<String>,
}

impl Store {
    pub(crate) async fn library_projects(&self) -> Result<Value> {
        let projects = self.list_projects().await?;
        let counts: Vec<(String, i64)> = sqlx::query_as("SELECT coalesce(project_id,''),count(*) FROM project_assets WHERE archived_at IS NULL GROUP BY project_id")
            .fetch_all(&self.pool).await?;
        let count = |id: &str| {
            counts
                .iter()
                .find(|(project, _)| project == id)
                .map(|(_, count)| *count)
                .unwrap_or(0)
        };
        let mut items: Vec<Value> = projects.iter().map(|project| json!({ "id": project.id, "name": project.name, "asset_count": count(&project.id) })).collect();
        items.push(json!({ "id": "", "name": "未归属项目", "asset_count": count("") }));
        Ok(json!(items))
    }

    pub(crate) async fn library_formats(&self, project: &str) -> Result<Value> {
        if !project.is_empty() {
            self.get_project(project).await?;
        }
        let formats: Vec<String> = sqlx::query_scalar("SELECT DISTINCT v.format FROM project_assets p JOIN asset_versions v ON v.id=p.asset_version_id WHERE coalesce(p.project_id,'')=? AND p.archived_at IS NULL ORDER BY v.format")
            .bind(project).fetch_all(&self.pool).await?;
        Ok(json!(formats))
    }

    pub(crate) async fn set_project_asset_version(&self, id: &str, version: &str) -> Result<Value> {
        let existing = self.project_asset(id).await?;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM asset_versions WHERE asset_id=? AND id=?")
                .bind(text(&existing, "asset_id")?)
                .bind(version)
                .fetch_one(&self.pool)
                .await?;
        if count != 1 {
            return Err(Error::NotFound);
        }
        sqlx::query("UPDATE project_assets SET asset_version_id=?,updated_at=? WHERE id=?")
            .bind(version)
            .bind(timestamp()?)
            .bind(id)
            .execute(&self.pool)
            .await?;
        self.project_asset(id).await
    }

    pub(crate) async fn copy_project_asset(
        &self,
        source: &str,
        project: &str,
        version: Option<&str>,
    ) -> Result<Value> {
        let source = self.project_asset(source).await?;
        let source_project = text(&source, "project_id")?;
        if source_project == project {
            return Err(Error::Invalid("请选择其他项目"));
        }
        if !project.is_empty() {
            self.get_project(project).await?;
        }
        let version = version.unwrap_or(text(&source, "asset_version_id")?);
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM asset_versions WHERE asset_id=? AND id=?")
                .bind(text(&source, "asset_id")?)
                .bind(version)
                .fetch_one(&self.pool)
                .await?;
        if count != 1 {
            return Err(Error::NotFound);
        }
        let name = if source_project.is_empty() {
            String::new()
        } else {
            self.get_project(source_project).await?.name
        };
        let origin = json!({ "project_id": source_project, "project_asset_id": source["id"], "project_name_snapshot": name, "display_name": source["display_name"], "deleted": false });
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        sqlx::query("INSERT INTO project_assets(id,project_id,asset_id,asset_version_id,display_name,added_at,updated_at,copy_source) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&id).bind(if project.is_empty() { None } else { Some(project) })
            .bind(text(&source, "asset_id")?).bind(version).bind(text(&source, "display_name")?)
            .bind(&now).bind(&now).bind(origin.to_string()).execute(&self.pool).await?;
        self.project_asset(&id).await
    }

    pub(crate) async fn project_asset_duplicates(&self, id: &str) -> Result<Value> {
        let source = self.project_asset(id).await?;
        let ids: Vec<String> = sqlx::query_scalar("SELECT p.id FROM project_assets p JOIN asset_versions v ON v.id=p.asset_version_id LEFT JOIN projects pr ON pr.id=p.project_id WHERE p.asset_id<>? AND v.digest=(SELECT digest FROM asset_versions WHERE id=?) ORDER BY (coalesce(p.project_id,'')=?) DESC,coalesce(pr.name,''),p.id")
            .bind(text(&source, "asset_id")?).bind(text(&source, "asset_version_id")?)
            .bind(text(&source, "project_id")?).fetch_all(&self.pool).await?;
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            items.push(self.project_asset(&id).await?);
        }
        Ok(json!({ "items": items }))
    }

    pub(crate) async fn library_batch(&self, batch: LibraryBatch) -> Result<Value> {
        if batch.project_asset_ids.is_empty() || batch.project_asset_ids.len() > 100 {
            return Err(Error::Invalid("请选择 1 至 100 项资产"));
        }
        let patch = match batch.action.as_str() {
            "category" => {
                json!({ "category_id": batch.category_id.as_deref().ok_or(Error::Invalid("缺少分类标识"))? })
            }
            "rating" => json!({ "rating": batch.rating.ok_or(Error::Invalid("缺少评分"))? }),
            "archive" => {
                json!({ "archived": batch.archived.ok_or(Error::Invalid("缺少归档状态"))? })
            }
            "tags" | "tags_remove" => {
                if batch.tag_ids.is_none() {
                    return Err(Error::Invalid("缺少标签列表"));
                }
                Value::Null
            }
            "project" => {
                if batch.project_id.is_none() {
                    return Err(Error::Invalid("缺少项目标识"));
                }
                Value::Null
            }
            _ => return Err(Error::Invalid("批量操作无效")),
        };
        let mut results = Vec::with_capacity(batch.project_asset_ids.len());
        for id in &batch.project_asset_ids {
            let result = match batch.action.as_str() {
                "category" | "rating" | "archive" => self.patch_project_asset(id, &patch).await,
                "project" => {
                    self.copy_project_asset(
                        id,
                        batch
                            .project_id
                            .as_deref()
                            .ok_or(Error::Invalid("缺少项目标识"))?,
                        None,
                    )
                    .await
                }
                _ => {
                    self.batch_tags(
                        id,
                        batch
                            .tag_ids
                            .as_deref()
                            .ok_or(Error::Invalid("缺少标签列表"))?,
                        batch.action == "tags_remove",
                    )
                    .await
                }
            };
            results.push(match result {
                Ok(_) => json!({ "project_asset_id": id, "success": true }),
                Err(error) => {
                    json!({ "project_asset_id": id, "success": false, "error": error.to_string() })
                }
            });
        }
        Ok(json!({ "results": results }))
    }

    async fn batch_tags(&self, id: &str, selected: &[String], remove: bool) -> Result<Value> {
        let existing = self.project_asset(id).await?;
        let mut tags = existing["tags"]
            .as_array()
            .ok_or(Error::State("资产标签列表无效"))?
            .iter()
            .map(|tag| text(tag, "id").map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        if remove {
            tags.retain(|id| !selected.contains(id));
        } else {
            for id in selected {
                if !tags.contains(id) {
                    tags.push(id.clone());
                }
            }
        }
        self.set_project_asset_tags(id, &tags).await
    }
}
