use crate::{
    Error, Result, Store,
    error::{required, timestamp},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: String,
    pub project_id: Option<String>,
    pub title: String,
    pub permission_mode: String,
    pub model_config_id: Option<String>,
    pub status: String,
    pub expert_id: Option<String>,
    pub expert_version: Option<String>,
    pub expert_name: Option<String>,
    pub current_expert_task_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const SESSION_FIELDS: &str = "id,project_id,title,permission_mode,model_config_id,status,expert_id,expert_version,expert_name,current_expert_task_id,created_at,updated_at";

impl Store {
    pub async fn clear_sessions(&self, account: &str) -> Result<()> {
        Self::check_account(account)?;
        let _model_update = self.model_updates.lock().await;
        let mut transaction = self.pool.begin().await?;
        let unfinished: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE account_id=? AND status NOT IN ('succeeded','failed','cancelled'))")
            .bind(account).fetch_one(&mut *transaction).await?;
        if unfinished {
            return Err(Error::Invalid("需要停止当前账号的运行后清空会话"));
        }
        let references: Vec<String> = sqlx::query_scalar("SELECT json_extract(expert_snapshot,'$.runtime_model_snapshot.credential_ref') FROM sessions WHERE account_id=? AND json_extract(expert_snapshot,'$.runtime_model_snapshot.credential_ref') IS NOT NULL UNION SELECT json_extract(model_snapshot,'$.credential_ref') FROM runs WHERE account_id=? AND json_extract(model_snapshot,'$.credential_ref') IS NOT NULL")
            .bind(account).bind(account).fetch_all(&mut *transaction).await?;
        sqlx::query("DELETE FROM sessions WHERE account_id=?")
            .bind(account)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        for reference in references {
            let retained: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM model_configs WHERE account_id=? AND credential_ref=?) OR EXISTS(SELECT 1 FROM sessions WHERE account_id=? AND json_extract(expert_snapshot,'$.runtime_model_snapshot.credential_ref')=?)")
                .bind(account).bind(&reference).bind(account).bind(&reference).fetch_one(&self.pool).await?;
            if !retained {
                crate::credentials::remove(&reference)?;
            }
        }
        Ok(())
    }

    pub async fn create_session(&self, account: &str, request_id: &str) -> Result<Session> {
        Self::check_account(account)?;
        required(request_id, "请求标识不能为空")?;
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        sqlx::query("INSERT INTO sessions(id,account_id,request_id,title,created_at,updated_at) VALUES(?,?,?,'新对话',?,?) ON CONFLICT(account_id,request_id) DO NOTHING")
            .bind(id).bind(account).bind(request_id).bind(&now).bind(&now).execute(&self.pool).await?;
        Ok(sqlx::query_as(&format!(
            "SELECT {SESSION_FIELDS} FROM sessions WHERE account_id=? AND request_id=?"
        ))
        .bind(account)
        .bind(request_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn get_session(&self, account: &str, id: &str) -> Result<Session> {
        Self::check_account(account)?;
        sqlx::query_as(&format!(
            "SELECT {SESSION_FIELDS} FROM sessions WHERE account_id=? AND id=?"
        ))
        .bind(account)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)
    }

    pub async fn list_sessions(
        &self,
        account: &str,
        project_id: Option<&str>,
        unassigned: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Session>> {
        Self::check_account(account)?;
        if !(1..=100).contains(&limit) || offset < 0 {
            return Err(Error::Invalid("分页参数无效"));
        }
        Ok(sqlx::query_as(&format!("SELECT {SESSION_FIELDS} FROM sessions WHERE account_id=? AND (? IS NULL OR project_id=?) AND (?=0 OR project_id IS NULL) ORDER BY updated_at DESC,id LIMIT ? OFFSET ?"))
            .bind(account).bind(project_id).bind(project_id).bind(unassigned).bind(limit).bind(offset).fetch_all(&self.pool).await?)
    }

    pub async fn move_session(
        &self,
        account: &str,
        id: &str,
        project_id: Option<&str>,
    ) -> Result<Session> {
        Self::check_account(account)?;
        if let Some(project) = project_id {
            self.get_project(account, project).await?;
        }
        let changed = sqlx::query(
            "UPDATE sessions SET project_id=?,updated_at=? WHERE account_id=? AND id=?",
        )
        .bind(project_id)
        .bind(timestamp()?)
        .bind(account)
        .bind(id)
        .execute(&self.pool)
        .await?;
        if changed.rows_affected() == 0 {
            return Err(Error::NotFound);
        }
        self.get_session(account, id).await
    }
}
