use crate::{
    Error, Result, Store,
    assets::digest,
    error::{required, timestamp},
    workflow_definitions::{ExecutionDefinition, WorkflowDefinition},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, Sqlite, Transaction, sqlite::SqliteRow};
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateExecution {
    request_id: String,
    workflow_id: String,
    version_id: Option<String>,
    draft_revision: Option<i64>,
    session_id: Option<String>,
    inputs: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionEvent {
    pub attempt_id: String,
    pub sequence: i64,
    pub kind: String,
    pub payload: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionCompletion {
    pub attempt_id: String,
    pub status: String,
    #[serde(default)]
    pub outputs: Vec<CompletedOutput>,
    #[serde(default)]
    pub error: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedOutput {
    pub output_id: String,
    pub sha256: String,
}

pub(crate) fn terminal(status: &str) -> bool {
    matches!(status, "succeeded" | "failed" | "cancelled" | "result_unknown")
}

async fn authorized_execution(
    transaction: &mut Transaction<'_, Sqlite>,
    token: &str,
    id: &str,
) -> Result<SqliteRow> {
    let node: String = sqlx::query_scalar("SELECT id FROM compute_nodes WHERE credential_hash=?")
        .bind(digest(token.as_bytes()))
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or(Error::NodeUnauthorized)?;
    let row = sqlx::query("SELECT * FROM local_executions WHERE id=?")
        .bind(id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or(Error::NotFound)?;
    if row.get::<String, _>("node_id") != node
        || row.get::<Option<String>, _>("attempt_id").is_none()
    {
        return Err(Error::Remote {
            status: 403,
            code: 403,
            message: "执行不属于当前计算节点".to_owned(),
        });
    }
    Ok(row)
}

fn check_attempt(row: &SqliteRow, attempt: &str) -> Result<()> {
    if row.get::<Option<String>, _>("attempt_id").as_deref() != Some(attempt) {
        return Err(Error::Conflict("执行尝试编号不匹配".to_owned()));
    }
    Ok(())
}

impl Store {
    pub async fn create_local_execution(&self, mut request: Value) -> Result<Value> {
        if let Some(workflow) = request["workflow_id"].as_str() {
            let kind: Option<String> = sqlx::query_scalar("SELECT type FROM workflows WHERE id=?").bind(workflow).fetch_optional(&self.pool).await?;
            if kind.as_deref() == Some("online") { return self.create_online_execution(request).await; }
        }
        request.sort_all_objects();
        let fingerprint = digest(&serde_json::to_vec(&request)?);
        let input: CreateExecution = serde_json::from_value(request)
            .map_err(|error| Error::Validation(error.to_string()))?;
        required(&input.request_id, "请求标识不能为空")?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(row) =
            sqlx::query("SELECT id,request_digest FROM local_executions WHERE request_id=?")
                .bind(&input.request_id)
                .fetch_optional(&mut *transaction)
                .await?
        {
            if row.get::<String, _>("request_digest") != fingerprint {
                return Err(Error::Conflict("请求标识已用于其他执行内容".to_owned()));
            }
            let id: String = row.get("id");
            transaction.commit().await?;
            return self.get_local_execution(&id).await;
        }
        let workflow = sqlx::query("SELECT * FROM workflows WHERE id=?")
            .bind(&input.workflow_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(Error::NotFound)?;
        if !workflow.get::<bool, _>("enabled") {
            return Err(Error::Invalid("工作流已停用"));
        }
        if input.version_id.is_some() && input.draft_revision.is_some() {
            return Err(Error::Invalid("执行版本与草稿修订不能同时指定"));
        }
        let version = input.version_id.or_else(|| {
            if input.draft_revision.is_none() {
                workflow.get("default_version_id")
            } else {
                None
            }
        });
        let definition: String = if let Some(version) = &version {
            sqlx::query_scalar(
                "SELECT definition FROM workflow_versions WHERE workflow_id=? AND id=?",
            )
            .bind(&input.workflow_id)
            .bind(version)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(Error::NotFound)?
        } else {
            if input.draft_revision != Some(workflow.get("revision")) {
                return Err(Error::Conflict("请选择工作流版本或当前草稿修订".to_owned()));
            }
            workflow.get("draft")
        };
        let definition = WorkflowDefinition::parse(serde_json::from_str(&definition)?)?;
        let ExecutionDefinition::Comfyui { execution } = &definition.execution else {
            return Err(Error::Invalid("此接口需要 ComfyUI 工作流"));
        };
        let node = self.get_compute_node(&execution.node_id).await?;
        if node["healthy"] != true {
            return Err(Error::Conflict("计算节点尚未就绪".to_owned()));
        }
        let validator = jsonschema::validator_for(&definition.input_schema)
            .map_err(|_| Error::State("工作流输入 Schema 无效"))?;
        if let Err(error) = validator.validate(&input.inputs) {
            return Err(Error::Validation(format!(
                "inputs{}：{error}",
                error.instance_path()
            )));
        }
        let values = input
            .inputs
            .as_object()
            .ok_or(Error::Invalid("工作流输入需要对象"))?;
        for field in &definition.inputs {
            if field.required
                && values.get(&field.key).is_none_or(|value| {
                    value.is_null() || value.as_str().is_some_and(|value| value.trim().is_empty())
                })
            {
                return Err(Error::Validation(format!("{}：请输入必要参数", field.key)));
            }
        }
        if let Some(session) = &input.session_id {
            self.get_session(session).await?;
        }
        let id = Uuid::new_v4().to_string();
        let mut graph = serde_json::to_value(&execution.workflow)?;
        let mut files = Vec::new();
        for binding in &execution.input_bindings {
            let Some(value) = values.get(&binding.key).filter(|value| !value.is_null()) else {
                continue;
            };
            let field = definition
                .inputs
                .iter()
                .find(|field| field.key == binding.key)
                .ok_or(Error::State("输入字段缺失"))?;
            if matches!(field.r#type.as_str(), "image" | "video" | "audio" | "file") {
                let asset = crate::local_api::text(value, "asset_id")?;
                let version = crate::local_api::text(value, "asset_version_id")?;
                let file = sqlx::query("SELECT v.digest,v.size_bytes,v.mime_type FROM asset_versions v WHERE v.asset_id=? AND v.id=?")
                    .bind(asset).bind(version).fetch_optional(&mut *transaction).await?.ok_or(Error::NotFound)?;
                let media_type: String = file.get("mime_type");
                if field.r#type != "file" && !media_type.starts_with(&format!("{}/", field.r#type))
                {
                    return Err(Error::Validation(format!(
                        "{}：素材格式与输入类型不一致",
                        field.key
                    )));
                }
                let file_id = Uuid::new_v4().to_string();
                files.push(json!({ "file_id": file_id, "asset_id": asset, "asset_version_id": version,
                    "size_bytes": file.get::<i64, _>("size_bytes"), "sha256": file.get::<String, _>("digest"),
                    "media_type": file.get::<String, _>("mime_type"), "field_key": binding.key,
                    "node_id": binding.node_id, "field_path": binding.field_path,
                    "url": format!("/agent/v1/executions/{id}/inputs/{file_id}") }));
            } else {
                graph[&binding.node_id]["inputs"][&binding.field_path] = value.clone();
            }
        }
        let mut package = json!({ "protocol_version": 1, "execution_id": id, "workflow_version_id": version,
            "workflow": graph, "inputs": input.inputs, "outputs": execution.output_bindings,
            "comfyui_base_url": node["comfyui_base_url"], "configuration_revision": node["configuration_revision"],
            "output_fields": definition.outputs, "input_files": files });
        package.sort_all_objects();
        let content_digest = digest(&serde_json::to_vec(&package)?);
        package["content_digest"] = json!(content_digest);
        let now = timestamp()?;
        sqlx::query("INSERT INTO local_executions(id,request_id,request_digest,workflow_id,workflow_version_id,node_id,session_id,definition,package,content_digest,status,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,'pending',?,?)")
            .bind(&id).bind(input.request_id).bind(fingerprint).bind(input.workflow_id).bind(version).bind(&execution.node_id)
            .bind(input.session_id).bind(serde_json::to_string(&definition)?).bind(package.to_string()).bind(content_digest).bind(&now).bind(&now)
            .execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_local_execution(&id).await
    }

    pub async fn get_local_execution(&self, id: &str) -> Result<Value> {
        if self.is_online_execution(id).await? {
            let result = self.refresh_online_execution(id).await;
            if let Err(Error::Http(error)) = &result {
                sqlx::query("UPDATE online_executions SET error=?,updated_at=? WHERE id=? AND status IN ('pending','running','collecting')")
                    .bind(format!("读取生成状态失败：{error}")).bind(timestamp()?).bind(id).execute(&self.pool).await?;
            }
            return result;
        }
        let row = sqlx::query("SELECT * FROM local_executions WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        let outputs = sqlx::query("SELECT * FROM local_execution_outputs WHERE execution_id=? ORDER BY field_key,output_index")
            .bind(id).fetch_all(&self.pool).await?;
        let outputs: Vec<Value> = outputs.iter().map(|row| json!({
            "output_id": row.get::<String, _>("output_id"), "field_key": row.get::<String, _>("field_key"),
            "index": row.get::<i64, _>("output_index"), "sha256": row.get::<String, _>("sha256"),
            "size_bytes": row.get::<i64, _>("size_bytes"), "media_type": row.get::<String, _>("media_type"),
            "asset_id": row.get::<String, _>("asset_id"), "asset_version_id": row.get::<String, _>("asset_version_id")
        })).collect();
        let event_rows = sqlx::query("SELECT document,created_at FROM local_execution_events WHERE execution_id=? ORDER BY sequence").bind(id).fetch_all(&self.pool).await?;
        let mut events = Vec::new();
        for event in event_rows {
            let mut document: Value = serde_json::from_str(&event.get::<String, _>("document"))?;
            document["created_at"] = json!(event.get::<String, _>("created_at"));
            events.push(document);
        }
        let package: Value = serde_json::from_str(&row.get::<String, _>("package"))?;
        Ok(
            json!({ "id": id, "workflow_id": row.get::<String, _>("workflow_id"),
            "workflow_version_id": row.get::<Option<String>, _>("workflow_version_id"), "node_id": row.get::<String, _>("node_id"),
            "session_id": row.get::<Option<String>, _>("session_id"), "status": if row.get::<bool,_>("result_unknown") {"result_unknown".to_owned()} else {row.get::<String, _>("status")},
            "attempt_id": row.get::<Option<String>, _>("attempt_id"), "prompt_id": row.get::<Option<String>, _>("prompt_id"),
            "sequence": row.get::<i64, _>("sequence"), "cancel_requested": row.get::<bool, _>("cancel_requested"),
            "waiting": row.get::<bool, _>("waiting"), "error": row.get::<String, _>("error"), "outputs": outputs,
            "events": events, "inputs": package["inputs"],
            "created_at": row.get::<String, _>("created_at"), "updated_at": row.get::<String, _>("updated_at") }),
        )
    }

    pub async fn list_local_executions(
        &self,
        workflow: Option<&str>,
        session: Option<&str>,
    ) -> Result<Value> {
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM local_executions WHERE (? IS NULL OR workflow_id=?) AND (? IS NULL OR session_id=?) ORDER BY created_at DESC,id")
            .bind(workflow).bind(workflow).bind(session).bind(session).fetch_all(&self.pool).await?;
        let mut result = Vec::new();
        for id in ids {
            result.push(self.get_local_execution(&id).await?);
        }
        result.extend(self.list_online_execution_documents(workflow, session).await?);
        Ok(json!(result))
    }

    pub(crate) async fn claim_local_execution(&self, token: &str) -> Result<Option<Value>> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let node = sqlx::query("SELECT id,enabled FROM compute_nodes WHERE credential_hash=?")
            .bind(digest(token.as_bytes()))
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(Error::NodeUnauthorized)?;
        let node_id: String = node.get("id");
        let active = sqlx::query("SELECT id,attempt_id FROM local_executions WHERE node_id=? AND attempt_id IS NOT NULL AND status NOT IN ('succeeded','failed','cancelled','result_unknown') ORDER BY created_at,id LIMIT 1")
            .bind(&node_id).fetch_optional(&mut *transaction).await?;
        let (id, attempt) = if let Some(row) = active {
            (
                row.get::<String, _>("id"),
                row.get::<String, _>("attempt_id"),
            )
        } else {
            if !node.get::<bool, _>("enabled")
                || self.get_compute_node(&node_id).await?["healthy"] != true
            {
                return Ok(None);
            }
            let id: Option<String> = sqlx::query_scalar("SELECT id FROM local_executions WHERE node_id=? AND status='pending' ORDER BY created_at,id LIMIT 1")
                .bind(&node_id).fetch_optional(&mut *transaction).await?;
            let Some(id) = id else {
                return Ok(None);
            };
            let attempt = Uuid::new_v4().to_string();
            sqlx::query("UPDATE local_executions SET attempt_id=?,status='dispatching',heartbeat_at=?,updated_at=? WHERE id=?")
                .bind(&attempt).bind(timestamp()?).bind(timestamp()?).bind(&id).execute(&mut *transaction).await?;
            (id, attempt)
        };
        transaction.commit().await?;
        Ok(Some(
            json!({ "protocol_version": 1, "execution_id": id, "attempt_id": attempt,
            "package_url": format!("/agent/v1/executions/{id}/package") }),
        ))
    }

    pub(crate) async fn local_execution_package(&self, token: &str, id: &str) -> Result<Value> {
        let mut transaction = self.pool.begin().await?;
        let row = authorized_execution(&mut transaction, token, id).await?;
        let mut package: Value = serde_json::from_str(row.get("package"))?;
        package["attempt_id"] = json!(row.get::<String, _>("attempt_id"));
        package["status"] = json!(row.get::<String, _>("status"));
        package["sequence"] = json!(row.get::<i64, _>("sequence"));
        package["prompt_id"] = json!(row.get::<Option<String>, _>("prompt_id"));
        Ok(package)
    }

    pub(crate) async fn local_execution_heartbeat(
        &self,
        token: &str,
        id: &str,
        attempt: &str,
    ) -> Result<Value> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = authorized_execution(&mut transaction, token, id).await?;
        check_attempt(&row, attempt)?;
        sqlx::query("UPDATE local_executions SET heartbeat_at=? WHERE id=?")
            .bind(timestamp()?)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(
            json!({ "cancel_requested": row.get::<bool, _>("cancel_requested"), "status": row.get::<String, _>("status") }),
        )
    }

    pub(crate) async fn local_execution_event(
        &self,
        token: &str,
        id: &str,
        event: ExecutionEvent,
        mut document: Value,
    ) -> Result<Value> {
        document.sort_all_objects();
        let document = document.to_string();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = authorized_execution(&mut transaction, token, id).await?;
        check_attempt(&row, &event.attempt_id)?;
        if let Some(saved) = sqlx::query_scalar::<_, String>(
            "SELECT document FROM local_execution_events WHERE execution_id=? AND sequence=?",
        )
        .bind(id)
        .bind(event.sequence)
        .fetch_optional(&mut *transaction)
        .await?
        {
            if saved != document {
                return Err(Error::Conflict("事件序号已用于其他内容".to_owned()));
            }
            return Ok(json!({ "sequence": event.sequence }));
        }
        let current: String = row.get("status");
        let previous: i64 = row.get("sequence");
        let prompt: Option<String> = row.get("prompt_id");
        if terminal(&current) || event.sequence != previous + 1 {
            return Err(Error::Conflict("执行状态或事件序号无效".to_owned()));
        }
        let mut prompt_id = prompt.clone();
        let mut error = String::new();
        let next = match event.kind.as_str() {
            "accepted" if previous == 0 => {
                if event.payload["content_digest"] != row.get::<String, _>("content_digest") {
                    return Err(Error::Conflict("任务包摘要不匹配".to_owned()));
                }
                "dispatching"
            }
            "submission_started"
                if current == "dispatching"
                    && previous == 1
                    && !row.get::<bool, _>("cancel_requested") =>
            {
                if event.payload["content_digest"] != row.get::<String, _>("content_digest") {
                    return Err(Error::Conflict("任务包摘要不匹配".to_owned()));
                }
                "dispatching"
            }
            "submitted"
                if previous >= 2
                    && matches!(current.as_str(), "dispatching" | "submission_unknown")
                    && prompt.is_none() =>
            {
                let value = crate::local_api::text(&event.payload, "prompt_id")?;
                required(value, "ComfyUI 编号不能为空")?;
                prompt_id = Some(value.to_owned());
                "running"
            }
            "progress" if current == "running" => "running",
            "collecting" if current == "running" => "collecting",
            "submission_unknown" if current == "dispatching" && previous >= 2 => {
                error = crate::local_api::text(&event.payload, "error")?.to_owned();
                "submission_unknown"
            }
            "failed" => {
                error = crate::local_api::text(&event.payload, "error")?.to_owned();
                required(&error, "执行错误不能为空")?;
                "failed"
            }
            "cancelled" if row.get::<bool, _>("cancel_requested") => "cancelled",
            _ => return Err(Error::Conflict("当前阶段不接受此执行事件".to_owned())),
        };
        sqlx::query("INSERT INTO local_execution_events(execution_id,attempt_id,sequence,document,created_at) VALUES(?,?,?,?,?)")
            .bind(id).bind(event.attempt_id).bind(event.sequence).bind(document).bind(timestamp()?).execute(&mut *transaction).await?;
        sqlx::query("UPDATE local_executions SET status=?,sequence=?,prompt_id=?,error=?,updated_at=? WHERE id=?")
            .bind(next).bind(event.sequence).bind(prompt_id).bind(error).bind(timestamp()?).bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(json!({ "sequence": event.sequence }))
    }

    pub async fn cancel_local_execution(&self, id: &str, stop_waiting: bool) -> Result<Value> {
        if self.is_online_execution(id).await? { return self.cancel_online_execution(id, stop_waiting).await; }
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT status FROM local_executions WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(Error::NotFound)?;
        let status: String = row.get("status");
        if stop_waiting {
            sqlx::query("UPDATE local_executions SET waiting=0,updated_at=? WHERE id=?")
                .bind(timestamp()?)
                .bind(id)
                .execute(&mut *transaction)
                .await?;
        } else if !terminal(&status) {
            sqlx::query("UPDATE local_executions SET cancel_requested=1,status=CASE WHEN status='pending' THEN 'cancelled' ELSE status END,updated_at=? WHERE id=?")
                .bind(timestamp()?).bind(id).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        self.get_local_execution(id).await
    }

    pub(crate) async fn complete_local_execution(
        &self,
        token: &str,
        id: &str,
        input: ExecutionCompletion,
        mut document: Value,
    ) -> Result<Value> {
        document.sort_all_objects();
        let document = document.to_string();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = authorized_execution(&mut transaction, token, id).await?;
        check_attempt(&row, &input.attempt_id)?;
        if let Some(saved) = row.get::<Option<String>, _>("completion") {
            if saved != document {
                return Err(Error::Conflict("执行已有其他结束结果".to_owned()));
            }
            return Ok(json!({ "confirmed": true, "status": input.status }));
        }
        let status: String = row.get("status");
        if !matches!(input.status.as_str(), "succeeded" | "failed" | "cancelled" | "result_unknown")
            || terminal(&status)
        {
            return Err(Error::Conflict("执行不能在当前阶段结束".to_owned()));
        }
        if input.status == "succeeded" {
            if !matches!(status.as_str(), "running" | "collecting") {
                return Err(Error::Conflict("执行尚未提交".to_owned()));
            }
            let package: Value = serde_json::from_str(row.get("package"))?;
            let saved = sqlx::query("SELECT output_id,field_key,sha256 FROM local_execution_outputs WHERE execution_id=? AND output_id NOT LIKE 'boundary:%'")
                .bind(id).fetch_all(&mut *transaction).await?;
            let unique: std::collections::HashSet<_> = input
                .outputs
                .iter()
                .map(|output| &output.output_id)
                .collect();
            if unique.len() != input.outputs.len() || saved.len() != input.outputs.len() {
                return Err(Error::Conflict("结束清单与已保存输出不一致".to_owned()));
            }
            for output in &input.outputs {
                if !saved.iter().any(|row| {
                    row.get::<String, _>("output_id") == output.output_id
                        && row.get::<String, _>("sha256") == output.sha256
                }) {
                    return Err(Error::Conflict("输出尚未确认或摘要不匹配".to_owned()));
                }
            }
            for output in package["outputs"]
                .as_array()
                .ok_or(Error::State("任务输出缺失"))?
            {
                if !saved
                    .iter()
                    .any(|row| row.get::<String, _>("field_key") == output["key"])
                {
                    return Err(Error::Conflict("必要输出尚未保存".to_owned()));
                }
            }
        } else {
            required(&input.error, "执行结束需要具体原因")?;
            if input.status == "cancelled" && !row.get::<bool, _>("cancel_requested") {
                return Err(Error::Conflict("执行尚未请求取消".to_owned()));
            }
        }
        sqlx::query(
            "UPDATE local_executions SET status=?,result_unknown=?,error=?,completion=?,updated_at=? WHERE id=?",
        )
        .bind(if input.status == "result_unknown" {"failed"} else {&input.status})
        .bind(input.status == "result_unknown")
        .bind(input.error)
        .bind(document)
        .bind(timestamp()?)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(json!({ "confirmed": true, "status": input.status }))
    }
}
