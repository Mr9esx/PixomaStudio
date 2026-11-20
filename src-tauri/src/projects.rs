use crate::{
    Error, Result, Store,
    error::{required, timestamp},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub project_type: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Store {
    pub async fn create_project(&self, name: &str) -> Result<Project> {
        self.create_project_with_type(name, "general").await
    }

    pub async fn create_project_with_type(&self, name: &str, project_type: &str) -> Result<Project> {
        required(name, "项目名称不能为空")?;
        validate_project_type(project_type)?;
        let now = timestamp()?;
        let project = Project {
            id: Uuid::new_v4().to_string(),
            name: name.trim().to_owned(),
            project_type: project_type.to_owned(),
            created_at: now.clone(),
            updated_at: now,
        };
        sqlx::query("INSERT INTO projects(id,name,project_type,created_at,updated_at) VALUES(?,?,?,?,?)")
            .bind(&project.id)
            .bind(&project.name)
            .bind(&project.project_type)
            .bind(&project.created_at)
            .bind(&project.updated_at)
            .execute(&self.pool)
            .await?;
        Ok(project)
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        Ok(sqlx::query_as(
            "SELECT id,name,project_type,created_at,updated_at FROM projects ORDER BY created_at DESC,id",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_project(&self, id: &str) -> Result<Project> {
        sqlx::query_as("SELECT id,name,project_type,created_at,updated_at FROM projects WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)
    }

    pub async fn update_project(&self, id: &str, name: &str, project_type: Option<&str>) -> Result<Project> {
        required(name, "项目名称不能为空")?;
        if let Some(project_type) = project_type {
            validate_project_type(project_type)?;
        }
        let changed = sqlx::query("UPDATE projects SET name=?,project_type=coalesce(?,project_type),updated_at=? WHERE id=?")
            .bind(name.trim())
            .bind(project_type)
            .bind(timestamp()?)
            .bind(id)
            .execute(&self.pool)
            .await?;
        if changed.rows_affected() == 0 {
            return Err(Error::NotFound);
        }
        self.get_project(id).await
    }

    pub async fn delete_project(&self, id: &str) -> Result<()> {
        use sqlx::Row;
        let mut transaction = self.pool.begin().await?;
        let collisions = sqlx::query("SELECT source.id AS source_id,target.id AS target_id FROM project_assets source JOIN project_assets target ON target.asset_id=source.asset_id AND target.project_id IS NULL WHERE source.project_id=?")
            .bind(id).fetch_all(&mut *transaction).await?;
        for collision in collisions {
            let source: String = collision.get("source_id");
            let target: String = collision.get("target_id");
            sqlx::query("INSERT INTO project_asset_tags(project_asset_id,tag_id) SELECT ?,tag_id FROM project_asset_tags WHERE project_asset_id=? ON CONFLICT DO NOTHING")
                .bind(target).bind(&source).execute(&mut *transaction).await?;
            sqlx::query("DELETE FROM project_assets WHERE id=?")
                .bind(source)
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query("UPDATE project_assets SET copy_source=json_set(copy_source,'$.deleted',json('true')) WHERE json_extract(copy_source,'$.project_id')=?")
            .bind(id).execute(&mut *transaction).await?;
        let changed = sqlx::query("DELETE FROM projects WHERE id=?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        if changed.rows_affected() == 0 {
            return Err(Error::NotFound);
        }
        transaction.commit().await?;
        Ok(())
    }
}

fn validate_project_type(project_type: &str) -> Result<()> {
    if !matches!(project_type, "general" | "video") {
        return Err(Error::Invalid("项目类型无效"));
    }
    Ok(())
}
