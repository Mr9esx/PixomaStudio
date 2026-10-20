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
    pub created_at: String,
    pub updated_at: String,
}

impl Store {
    pub async fn create_project(&self, account: &str, name: &str) -> Result<Project> {
        Self::check_account(account)?;
        required(name, "项目名称不能为空")?;
        let now = timestamp()?;
        let project = Project {
            id: Uuid::new_v4().to_string(),
            name: name.trim().to_owned(),
            created_at: now.clone(),
            updated_at: now,
        };
        sqlx::query(
            "INSERT INTO projects(id,account_id,name,created_at,updated_at) VALUES(?,?,?,?,?)",
        )
        .bind(&project.id)
        .bind(account)
        .bind(&project.name)
        .bind(&project.created_at)
        .bind(&project.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(project)
    }

    pub async fn list_projects(&self, account: &str) -> Result<Vec<Project>> {
        Self::check_account(account)?;
        Ok(sqlx::query_as("SELECT id,name,created_at,updated_at FROM projects WHERE account_id=? ORDER BY created_at DESC,id")
            .bind(account).fetch_all(&self.pool).await?)
    }

    pub async fn get_project(&self, account: &str, id: &str) -> Result<Project> {
        Self::check_account(account)?;
        sqlx::query_as(
            "SELECT id,name,created_at,updated_at FROM projects WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)
    }

    pub async fn rename_project(&self, account: &str, id: &str, name: &str) -> Result<Project> {
        Self::check_account(account)?;
        required(name, "项目名称不能为空")?;
        let changed =
            sqlx::query("UPDATE projects SET name=?,updated_at=? WHERE account_id=? AND id=?")
                .bind(name.trim())
                .bind(timestamp()?)
                .bind(account)
                .bind(id)
                .execute(&self.pool)
                .await?;
        if changed.rows_affected() == 0 {
            return Err(Error::NotFound);
        }
        self.get_project(account, id).await
    }

    pub async fn delete_project(&self, account: &str, id: &str) -> Result<()> {
        Self::check_account(account)?;
        use sqlx::Row;
        let mut transaction = self.pool.begin().await?;
        let collisions = sqlx::query("SELECT source.id AS source_id,target.id AS target_id FROM project_assets source JOIN project_assets target ON target.account_id=source.account_id AND target.asset_id=source.asset_id AND target.project_id IS NULL WHERE source.account_id=? AND source.project_id=?")
            .bind(account).bind(id).fetch_all(&mut *transaction).await?;
        for collision in collisions {
            let source: String = collision.get("source_id");
            let target: String = collision.get("target_id");
            sqlx::query("INSERT INTO project_asset_tags(account_id,project_asset_id,tag_id) SELECT account_id,?,tag_id FROM project_asset_tags WHERE account_id=? AND project_asset_id=? ON CONFLICT DO NOTHING")
                .bind(target).bind(account).bind(&source).execute(&mut *transaction).await?;
            sqlx::query("DELETE FROM project_assets WHERE account_id=? AND id=?")
                .bind(account)
                .bind(source)
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query("UPDATE project_assets SET copy_source=json_set(copy_source,'$.deleted',json('true')) WHERE account_id=? AND json_extract(copy_source,'$.project_id')=?")
            .bind(account).bind(id).execute(&mut *transaction).await?;
        let changed = sqlx::query("DELETE FROM projects WHERE account_id=? AND id=?")
            .bind(account)
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
