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
    pub async fn clear_sessions(&self) -> Result<()> {
        let _model_update = self.model_updates.lock().await;
        let mut transaction = self.pool.begin().await?;
        let unfinished: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE status NOT IN ('succeeded','failed','cancelled'))")
            .fetch_one(&mut *transaction).await?;
        if unfinished {
            return Err(Error::Invalid("需要停止当前运行后清空会话"));
        }
        sqlx::query("UPDATE local_executions SET session_id=NULL,waiting=CASE WHEN EXISTS(SELECT 1 FROM video_generations g WHERE g.execution_request_id=local_executions.request_id) THEN waiting ELSE 0 END WHERE session_id IS NOT NULL")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE online_executions SET session_id=NULL,waiting=CASE WHEN EXISTS(SELECT 1 FROM video_generations g WHERE g.execution_request_id=online_executions.request_id) THEN waiting ELSE 0 END WHERE session_id IS NOT NULL")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM sessions")
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn create_session(&self, request_id: &str) -> Result<Session> {
        required(request_id, "请求标识不能为空")?;
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        sqlx::query("INSERT INTO sessions(id,request_id,title,created_at,updated_at) VALUES(?,?,'新对话',?,?) ON CONFLICT(request_id) DO NOTHING")
            .bind(id).bind(request_id).bind(&now).bind(&now).execute(&self.pool).await?;
        Ok(sqlx::query_as(&format!(
            "SELECT {SESSION_FIELDS} FROM sessions WHERE request_id=?"
        ))
        .bind(request_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn get_session(&self, id: &str) -> Result<Session> {
        sqlx::query_as(&format!("SELECT {SESSION_FIELDS} FROM sessions WHERE id=?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)
    }

    pub async fn list_sessions(
        &self,
        project_id: Option<&str>,
        unassigned: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Session>> {
        if !(1..=100).contains(&limit) || offset < 0 {
            return Err(Error::Invalid("分页参数无效"));
        }
        Ok(sqlx::query_as(&format!("SELECT {SESSION_FIELDS} FROM sessions WHERE (? IS NULL OR project_id=?) AND (?=0 OR project_id IS NULL) ORDER BY updated_at DESC,id LIMIT ? OFFSET ?"))
            .bind(project_id).bind(project_id).bind(unassigned).bind(limit).bind(offset).fetch_all(&self.pool).await?)
    }

    pub async fn move_session(&self, id: &str, project_id: Option<&str>) -> Result<Session> {
        if let Some(project) = project_id {
            self.get_project(project).await?;
        }
        let changed = sqlx::query("UPDATE sessions SET project_id=?,updated_at=? WHERE id=?")
            .bind(project_id)
            .bind(timestamp()?)
            .bind(id)
            .execute(&self.pool)
            .await?;
        if changed.rows_affected() == 0 {
            return Err(Error::NotFound);
        }
        self.get_session(id).await
    }
}
