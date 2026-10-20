use crate::{
    Error, Result, Store,
    error::{required, timestamp},
    local_api::text,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, sqlx::FromRow)]
pub(crate) struct Category {
    pub id: String,
    pub project_id: String,
    pub parent_id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LibraryPreferences {
    tree_mode: String,
    last_project_id: String,
}

pub(crate) fn check_tree_mode(mode: &str) -> Result<()> {
    if !["asset", "session", "category", "format", "rating", "tag"].contains(&mode) {
        return Err(Error::Invalid("资产分组方式无效"));
    }
    Ok(())
}

impl Store {
    pub(crate) async fn category(&self, account: &str, id: &str) -> Result<Category> {
        Self::check_account(account)?;
        sqlx::query_as("SELECT id,coalesce(project_id,'') AS project_id,coalesce(parent_id,'') AS parent_id,name,created_at,updated_at FROM categories WHERE account_id=? AND id=?")
            .bind(account).bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)
    }

    pub(crate) async fn categories(&self, account: &str, project: &str) -> Result<Vec<Category>> {
        Self::check_account(account)?;
        if !project.is_empty() {
            self.get_project(account, project).await?;
        }
        Ok(sqlx::query_as("SELECT id,coalesce(project_id,'') AS project_id,coalesce(parent_id,'') AS parent_id,name,created_at,updated_at FROM categories WHERE account_id=? AND coalesce(project_id,'')=? ORDER BY name,id")
            .bind(account).bind(project).fetch_all(&self.pool).await?)
    }

    pub(crate) async fn update_category(
        &self,
        account: &str,
        id: &str,
        patch: &Value,
    ) -> Result<Value> {
        let category = self.category(account, id).await?;
        let fields = patch.as_object().ok_or(Error::Invalid("分类参数无效"))?;
        if fields
            .keys()
            .any(|key| !["name", "parent_id"].contains(&key.as_str()))
        {
            return Err(Error::Invalid("分类字段无效"));
        }
        let name = if fields.contains_key("name") {
            text(patch, "name")?
        } else {
            &category.name
        };
        required(name, "分类名称不能为空")?;
        if name.trim().chars().count() > 128 {
            return Err(Error::Invalid("分类名称不能超过 128 个字符"));
        }
        let parent = if fields.contains_key("parent_id") {
            text(patch, "parent_id")?
        } else {
            &category.parent_id
        };
        let mut transaction = self.pool.begin().await?;
        if !parent.is_empty() {
            let exists: i64 = sqlx::query_scalar("SELECT count(*) FROM categories WHERE account_id=? AND coalesce(project_id,'')=? AND id=?")
                .bind(account).bind(&category.project_id).bind(parent).fetch_one(&mut *transaction).await?;
            if exists != 1 {
                return Err(Error::NotFound);
            }
            let cycle: i64 = sqlx::query_scalar("WITH RECURSIVE descendants(id) AS (SELECT id FROM categories WHERE account_id=? AND id=? UNION SELECT c.id FROM categories c JOIN descendants d ON c.parent_id=d.id WHERE c.account_id=?) SELECT count(*) FROM descendants WHERE id=?")
                .bind(account).bind(id).bind(account).bind(parent).fetch_one(&mut *transaction).await?;
            if cycle != 0 {
                return Err(Error::Invalid("分类不能移到自身或下级分类"));
            }
        }
        sqlx::query(
            "UPDATE categories SET name=?,parent_id=?,updated_at=? WHERE account_id=? AND id=?",
        )
        .bind(name.trim())
        .bind(if parent.is_empty() {
            None
        } else {
            Some(parent)
        })
        .bind(timestamp()?)
        .bind(account)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(serde_json::to_value(self.category(account, id).await?)?)
    }

    pub(crate) async fn delete_category(&self, account: &str, id: &str) -> Result<()> {
        let category = self.category(account, id).await?;
        let parent = if category.parent_id.is_empty() {
            None
        } else {
            Some(category.parent_id.as_str())
        };
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "UPDATE categories SET parent_id=?,updated_at=? WHERE account_id=? AND parent_id=?",
        )
        .bind(parent)
        .bind(timestamp()?)
        .bind(account)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE project_assets SET category_id=?,updated_at=? WHERE account_id=? AND category_id=?")
            .bind(parent).bind(timestamp()?).bind(account).bind(id).execute(&mut *transaction).await?;
        sqlx::query("DELETE FROM categories WHERE account_id=? AND id=?")
            .bind(account)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn tags(&self, account: &str) -> Result<Value> {
        Self::check_account(account)?;
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT id,name FROM tags WHERE account_id=? ORDER BY name,id")
                .bind(account)
                .fetch_all(&self.pool)
                .await?;
        Ok(json!(
            rows.iter()
                .map(|(id, name)| json!({ "id": id, "name": name }))
                .collect::<Vec<_>>()
        ))
    }

    pub(crate) async fn library_preferences(&self, account: &str) -> Result<Value> {
        Self::check_account(account)?;
        let document: Option<String> =
            sqlx::query_scalar("SELECT document FROM preferences WHERE account_id=?")
                .bind(account)
                .fetch_optional(&self.pool)
                .await?;
        match document {
            Some(document) => Ok(serde_json::from_str(&document)?),
            None => Ok(json!({ "tree_mode": "asset", "last_project_id": "" })),
        }
    }

    pub(crate) async fn save_library_preferences(
        &self,
        account: &str,
        document: Value,
    ) -> Result<Value> {
        Self::check_account(account)?;
        let preferences: LibraryPreferences = serde_json::from_value(document)?;
        check_tree_mode(&preferences.tree_mode)?;
        if !preferences.last_project_id.is_empty() {
            self.get_project(account, &preferences.last_project_id)
                .await?;
        }
        let document = serde_json::to_value(preferences)?;
        sqlx::query("INSERT INTO preferences(account_id,document) VALUES(?,?) ON CONFLICT(account_id) DO UPDATE SET document=excluded.document")
            .bind(account).bind(document.to_string()).execute(&self.pool).await?;
        Ok(document)
    }
}
