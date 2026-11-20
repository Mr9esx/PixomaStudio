use crate::{Error, Result, Store, error::{required, timestamp}, local_api::text, workflow_definitions::WorkflowDefinition};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, Sqlite, Transaction, sqlite::SqliteRow};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GenerationInput {
    request_id: String,
    session_id: String,
    shot_id: String,
    expected_version: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RetryInput {
    request_id: String,
    session_id: String,
}

fn record(row: SqliteRow) -> Result<Value> {
    Ok(json!({
        "id":row.get::<String,_>("id"), "video_id":row.get::<String,_>("video_id"),
        "shot_id":row.get::<String,_>("shot_id"), "session_id":row.get::<Option<String>,_>("session_id"),
        "execution_request_id":row.get::<String,_>("execution_request_id"),
        "execution_id":row.get::<Option<String>,_>("execution_id"),
        "snapshot":serde_json::from_str::<Value>(&row.get::<String,_>("snapshot"))?,
        "retry_of":row.get::<Option<String>,_>("retry_of"),
        "submission_error":row.get::<Option<String>,_>("submission_error"),
        "dependency_state":row.get::<String,_>("dependency_state"),
        "dependency_error":row.get::<Option<String>,_>("dependency_error"),
        "dependencies":serde_json::from_str::<Value>(&row.get::<String,_>("dependencies"))?,
        "created_at":row.get::<String,_>("created_at")
    }))
}

pub(crate) async fn check_active(transaction: &mut Transaction<'_,Sqlite>, video: &str, shot: &str) -> Result<()> {
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM video_generations g LEFT JOIN local_executions l ON l.request_id=g.execution_request_id LEFT JOIN online_executions o ON o.request_id=g.execution_request_id WHERE g.video_id=? AND g.shot_id=? AND g.dependency_state NOT IN ('blocked','cancelled') AND g.submission_error IS NULL AND COALESCE(l.result_unknown,0)=0 AND COALESCE(l.waiting,o.waiting,1)=1 AND COALESCE(l.status,o.status,'submitting') NOT IN ('succeeded','failed','cancelled','result_unknown'))")
        .bind(video).bind(shot).fetch_one(&mut **transaction).await?;
    if active {return Err(Error::Conflict("此分镜已有生成任务，请等待当前任务结束".into()));}
    Ok(())
}

impl Store {
    pub(crate) async fn place_video_generation_output(&self, execution_id: &str, asset_id: &str, version_id: &str) -> Result<()> {
        let projects: Vec<Option<String>> = sqlx::query_scalar("SELECT DISTINCT w.project_id FROM video_generations g JOIN video_works w ON w.id=g.video_id WHERE g.execution_request_id IN (SELECT request_id FROM local_executions WHERE id=? UNION ALL SELECT request_id FROM online_executions WHERE id=?)")
            .bind(execution_id).bind(execution_id).fetch_all(&self.pool).await?;
        for project in projects {
            self.place_asset(project.as_deref().unwrap_or(""),asset_id,version_id).await?;
        }
        Ok(())
    }

    pub(crate) async fn authorize_video_generation(&self, video_id: &str, session_id: &str) -> Result<Value> {
        let work = self.read_video_work(video_id).await?;
        let session = self.get_session(session_id).await?;
        if work["main_session_id"] != session_id && !work["chats"].as_array().is_some_and(|chats|chats.iter().any(|chat|chat["session_id"]==session_id)) {
            return Err(Error::Invalid("当前对话没有关联此视频作品"));
        }
        if session.project_id.as_deref() != work["project_id"].as_str() {return Err(Error::Invalid("当前对话与视频作品所属项目不同"));}
        Ok(work)
    }

    pub async fn prepare_video_generation(&self, video_id: &str, request: Value) -> Result<Value> {
        let input: GenerationInput = serde_json::from_value(request)?;
        required(&input.request_id,"请求标识不能为空")?;
        let work = self.authorize_video_generation(video_id,&input.session_id).await?;
        self.preserve_legacy_video_generations(video_id,&work).await?;
        if let Some(row) = sqlx::query("SELECT * FROM video_generations WHERE video_id=? AND id=?").bind(video_id).bind(&input.request_id).fetch_optional(&self.pool).await? {
            let previous = record(row)?;
            if previous["shot_id"] != input.shot_id || previous["retry_of"].is_string() {return Err(Error::Conflict("请求标识已用于其他生成内容".into()));}
            return self.video_generation_document(previous).await;
        }
        if work["asset_version"] != input.expected_version {return Err(Error::Conflict("视频作品已更新，请读取最新内容后生成".into()));}
        let snapshot = self.video_generation_snapshot(&work,&input.shot_id,&input.request_id,input.expected_version).await?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(row) = sqlx::query("SELECT * FROM video_generations WHERE video_id=? AND id=?").bind(video_id).bind(&input.request_id).fetch_optional(&mut *transaction).await? {
            let previous = record(row)?;
            if previous["shot_id"] != input.shot_id || previous["retry_of"].is_string() {return Err(Error::Conflict("请求标识已用于其他生成内容".into()));}
            transaction.commit().await?;
            return self.video_generation_document(previous).await;
        }
        let current: i64 = sqlx::query_scalar("SELECT current_version FROM assets WHERE id=?").bind(video_id).fetch_one(&mut *transaction).await?;
        if current != input.expected_version {return Err(Error::Conflict("视频作品已更新，请读取最新内容后生成".into()));}
        check_active(&mut transaction,video_id,&input.shot_id).await?;
        sqlx::query("INSERT INTO video_generations(video_id,id,shot_id,session_id,execution_request_id,snapshot,created_at) VALUES(?,?,?,?,?,?,?)")
            .bind(video_id).bind(&input.request_id).bind(&input.shot_id).bind(&input.session_id)
            .bind(format!("video-generation:{video_id}:{}",input.request_id)).bind(snapshot.to_string()).bind(timestamp()?).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_video_generation(video_id,&input.request_id).await
    }

    pub(crate) async fn video_generation_snapshot(&self, work: &Value, shot_id: &str, request_id: &str, project_version: i64) -> Result<Value> {
        let shot = work["project"]["shots"].as_array().and_then(|shots|shots.iter().find(|shot|shot["id"]==shot_id)).ok_or(Error::Invalid("生成分镜不存在"))?;
        let settings = if shot["generationRequest"]["id"] == request_id {&shot["generationRequest"]} else {shot};
        let workflow_id = text(settings,"workflowId")?;
        let version_id = text(settings,"versionId")?;
        let version = self.get_workflow_version(workflow_id,version_id).await?;
        let definition = WorkflowDefinition::parse(version["definition"].clone())?;
        if !definition.outputs.iter().any(|field|field.r#type=="video") {return Err(Error::Invalid("请选择可以生成视频的工作流"));}
        let mut inputs = settings["inputs"].as_object().ok_or(Error::Invalid("工作流输入需要对象"))?.clone();
        for field in &definition.inputs {
            if !inputs.contains_key(&field.key) {
                if let Some(value) = definition.input_schema["properties"][&field.key].get("default") {inputs.insert(field.key.clone(),value.clone());}
            }
        }
        let workflow = self.get_workflow(workflow_id).await?;
        Ok(json!({"project_version":project_version,"shot":shot,"workflow_id":workflow_id,"workflow_name":workflow["name"],"version_id":version_id,"version_name":version["version"],"input_fields":definition.inputs,"inputs":inputs}))
    }

    async fn video_generation_execution(&self, request: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT id FROM local_executions WHERE request_id=? UNION ALL SELECT id FROM online_executions WHERE request_id=?")
            .bind(request).bind(request).fetch_optional(&self.pool).await?)
    }

    async fn video_generation_document(&self, mut value: Value) -> Result<Value> {
        if value["dependency_state"]!="ready" {
            value["status"] = json!(match value["dependency_state"].as_str() {Some("waiting_output")=>"waiting_output",Some("blocked")=>"blocked_dependencies",Some("cancelled")=>"cancelled",_=>"waiting_dependencies"});
            value["execution"] = Value::Null;
            return Ok(value);
        }
        let execution_id = self.video_generation_execution(text(&value,"execution_request_id")?).await?;
        if let Some(id) = execution_id {
            let execution = self.get_local_execution(&id).await?;
            for output in execution["outputs"].as_array().ok_or(Error::State("生成任务输出缺失"))? {
                self.place_video_generation_output(&id,text(output,"asset_id")?,text(output,"asset_version_id")?).await?;
            }
            value["execution_id"] = json!(id);
            value["status"] = execution["status"].clone();
            value["execution"] = execution;
            value["submission_error"] = Value::Null;
        } else {
            value["status"] = json!(if value["submission_error"].is_string() {"submission_failed"} else {"submitting"});
            value["execution"] = Value::Null;
        }
        Ok(value)
    }

    pub async fn get_video_generation(&self, video_id: &str, id: &str) -> Result<Value> {
        let row = sqlx::query("SELECT * FROM video_generations WHERE video_id=? AND id=?").bind(video_id).bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        self.video_generation_document(record(row)?).await
    }

    pub async fn list_video_generations(&self, video_id: &str) -> Result<Value> {
        let work = self.read_video_work(video_id).await?;
        self.preserve_legacy_video_generations(video_id,&work).await?;
        let rows = sqlx::query("SELECT * FROM video_generations WHERE video_id=? ORDER BY created_at DESC,id").bind(video_id).fetch_all(&self.pool).await?;
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {records.push(self.video_generation_document(record(row)?).await?);}
        Ok(json!(records))
    }

    pub(crate) async fn preserve_legacy_video_generations(&self, video_id: &str, work: &Value) -> Result<()> {
        let shots = work["project"]["shots"].as_array().ok_or(Error::State("视频分镜缺失"))?;
        for shot in shots {
            let mut execution_ids = std::collections::BTreeSet::new();
            if let Some(id) = shot["executionId"].as_str() {execution_ids.insert(id);}
            if let Some(versions) = shot["versions"].as_array() {
                for version in versions {if let Some(id) = version["executionId"].as_str() {execution_ids.insert(id);}}
            }
            for execution_id in execution_ids {
                let request: Option<String> = sqlx::query_scalar("SELECT request_id FROM local_executions WHERE id=? UNION ALL SELECT request_id FROM online_executions WHERE id=?")
                    .bind(execution_id).bind(execution_id).fetch_optional(&self.pool).await?;
                let Some(request) = request else {continue;};
                let recorded: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM video_generations WHERE execution_request_id=?)").bind(&request).fetch_one(&self.pool).await?;
                if recorded {continue;}
                let execution = self.get_local_execution(execution_id).await?;
                let workflow_id = text(&execution,"workflow_id")?;
                let version_id = text(&execution,"workflow_version_id")?;
                let workflow = self.get_workflow(workflow_id).await?;
                let version = self.get_workflow_version(workflow_id,version_id).await?;
                let snapshot = json!({"legacy":true,"project_version":null,"shot":{"id":shot["id"],"name":shot["name"]},"workflow_id":workflow_id,"workflow_name":workflow["name"],"version_id":version_id,"version_name":version["version"],"input_fields":version["definition"]["inputs"],"inputs":execution["inputs"]});
                sqlx::query("INSERT INTO video_generations(video_id,id,shot_id,session_id,execution_request_id,execution_id,snapshot,created_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
                    .bind(video_id).bind(format!("execution:{execution_id}")).bind(text(shot,"id")?).bind(execution["session_id"].as_str())
                    .bind(request).bind(execution_id).bind(snapshot.to_string()).bind(text(&execution,"created_at")?).execute(&self.pool).await?;
            }
        }
        Ok(())
    }

    pub async fn submit_video_generation(&self, video_id: &str, id: &str) -> Result<Value> {
        let row = sqlx::query("SELECT * FROM video_generations WHERE video_id=? AND id=?").bind(video_id).bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let value = record(row)?;
        if value["dependency_state"]!="ready" {return self.video_generation_document(value).await;}
        let request_id = text(&value,"execution_request_id")?;
        if let Some(execution_id) = self.video_generation_execution(request_id).await? {
            sqlx::query("UPDATE video_generations SET execution_id=?,submission_error=NULL WHERE video_id=? AND id=?")
                .bind(execution_id).bind(video_id).bind(id).execute(&self.pool).await?;
        } else if value["submission_error"].is_null() {
            let snapshot = &value["snapshot"];
            let result = self.create_local_execution(json!({"request_id":request_id,"workflow_id":snapshot["workflow_id"],"version_id":snapshot["version_id"],"session_id":value["session_id"],"inputs":snapshot["inputs"]})).await;
            match result {
                Ok(execution) => {
                    sqlx::query("UPDATE video_generations SET execution_id=?,submission_error=NULL WHERE video_id=? AND id=?")
                        .bind(text(&execution,"id")?).bind(video_id).bind(id).execute(&self.pool).await?;
                }
                Err(error) => {
                    if !matches!(error,Error::Invalid(_) | Error::Validation(_) | Error::Conflict(_) | Error::NotFound | Error::Http(_)) {return Err(error);}
                    if let Some(execution_id) = self.video_generation_execution(request_id).await? {
                        sqlx::query("UPDATE video_generations SET execution_id=?,submission_error=NULL WHERE video_id=? AND id=?")
                            .bind(execution_id).bind(video_id).bind(id).execute(&self.pool).await?;
                    } else {
                        sqlx::query("UPDATE video_generations SET submission_error=? WHERE video_id=? AND id=? AND execution_id IS NULL")
                            .bind(error.to_string()).bind(video_id).bind(id).execute(&self.pool).await?;
                    }
                }
            }
        }
        self.get_video_generation(video_id,id).await
    }

    pub async fn create_video_generation(&self, video_id: &str, input: Value) -> Result<Value> {
        let prepared = self.prepare_video_generation(video_id,input).await?;
        self.submit_video_generation(video_id,text(&prepared,"id")?).await
    }

    pub async fn retry_video_generation(&self, video_id: &str, original_id: &str, input: Value) -> Result<Value> {
        let input: RetryInput = serde_json::from_value(input)?;
        required(&input.request_id,"请求标识不能为空")?;
        self.authorize_video_generation(video_id,&input.session_id).await?;
        let original = self.get_video_generation(video_id,original_id).await?;
        if original["dependency_state"]!="ready" {return Err(Error::Invalid("此任务的依赖尚未完成，请重新确认生成范围"));}
        let shot_id = text(&original,"shot_id")?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(row) = sqlx::query("SELECT * FROM video_generations WHERE video_id=? AND id=?").bind(video_id).bind(&input.request_id).fetch_optional(&mut *transaction).await? {
            if row.get::<Option<String>,_>("retry_of").as_deref() != Some(original_id) {return Err(Error::Conflict("请求标识已用于其他生成内容".into()));}
        } else {
            check_active(&mut transaction,video_id,shot_id).await?;
            sqlx::query("INSERT INTO video_generations(video_id,id,shot_id,session_id,execution_request_id,snapshot,retry_of,created_at) VALUES(?,?,?,?,?,?,?,?)")
                .bind(video_id).bind(&input.request_id).bind(shot_id).bind(&input.session_id)
                .bind(format!("video-generation:{video_id}:{}",input.request_id)).bind(original["snapshot"].to_string()).bind(original_id).bind(timestamp()?).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        self.submit_video_generation(video_id,&input.request_id).await
    }

    pub async fn resume_video_generations(&self) -> Result<()> {
        self.resume_video_generation_dependencies().await?;
        let pending: Vec<(String,String)> = sqlx::query_as("SELECT video_id,id FROM video_generations WHERE dependency_state='ready' AND execution_id IS NULL AND submission_error IS NULL ORDER BY created_at,id").fetch_all(&self.pool).await?;
        for (video_id,id) in pending {self.submit_video_generation(&video_id,&id).await?;}
        let executions: Vec<String> = sqlx::query_scalar("SELECT e.id FROM online_executions e WHERE e.status IN ('pending','running','collecting') AND EXISTS(SELECT 1 FROM video_generations g WHERE g.execution_request_id=e.request_id) ORDER BY e.created_at,e.id")
            .fetch_all(&self.pool).await?;
        for id in executions {
            if let Err(error) = self.get_local_execution(&id).await {
                if !matches!(error,Error::Http(_)) { return Err(error); }
            }
        }
        Ok(())
    }
}
