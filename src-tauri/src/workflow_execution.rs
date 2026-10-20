use crate::{
    Error, Result, Store,
    error::timestamp,
    local_api::text,
    tools::{RunTool, ToolTarget},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rig::message::ToolCall;
use serde_json::{Value, json};
use std::time::Duration;

fn workflow_output_extension(
    bytes: &[u8],
    media_type: Option<&str>,
    textual: bool,
) -> Result<&'static str> {
    let media_type = media_type
        .map(str::parse::<mime_guess::Mime>)
        .transpose()
        .map_err(|_| Error::Invalid("工作流资源 MIME 类型无效"))?;
    if media_type.as_ref().is_some_and(|mime| {
        mime.type_() == mime_guess::mime::STAR || mime.subtype() == mime_guess::mime::STAR
    }) {
        return Err(Error::Invalid("工作流资源 MIME 类型需要具体格式"));
    }
    let extension = infer::get(bytes)
        .map(|format| format.extension())
        .or_else(|| {
            media_type
                .as_ref()
                .and_then(|mime| match mime.essence_str() {
                    "text/plain" => Some("txt"),
                    "application/octet-stream" => Some("bin"),
                    value => mime_guess::get_mime_extensions_str(value)
                        .and_then(|extensions| extensions.first().copied()),
                })
        });
    Ok(extension.unwrap_or(if textual { "txt" } else { "bin" }))
}

impl Store {
    async fn record_expert_workflow_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        execution: &Value,
    ) -> Result<()> {
        let Some(attempt_id) = execution["expert_step_attempt_id"].as_str() else {
            return Ok(());
        };
        let mut attempt = Self::expert_attempt_in(transaction, account, attempt_id).await?;
        if attempt["expert_task_id"] != execution["expert_task_id"] {
            return Err(Error::State("工作流与专家处理记录不一致"));
        }
        let original = attempt.clone();
        let mut task =
            Self::expert_task_in(transaction, account, text(&attempt, "expert_task_id")?).await?;
        if execution["submission_state"] == "submitting" {
            if !matches!(attempt["status"].as_str(), Some("running" | "waiting_tool")) {
                return Err(Error::Invalid("专家步骤尚未开始执行"));
            }
            if !matches!(task["status"].as_str(), Some("active" | "waiting_tool"))
                || task["current_step_id"] != attempt["step_id"]
                || !task["steps"]
                    .as_array()
                    .ok_or(Error::State("专家步骤格式无效"))?
                    .iter()
                    .any(|step| {
                        step["latest_attempt_id"] == attempt_id
                            && step["step_id"] == attempt["step_id"]
                            && matches!(step["status"].as_str(), Some("running" | "waiting_tool"))
                    })
            {
                return Err(Error::Invalid("当前专家步骤已经更改"));
            }
            attempt["status"] = json!("waiting_tool");
        } else if execution["status"] == "succeeded" && attempt["status"] == "waiting_tool" {
            let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workflow_executions WHERE account_id=? AND json_extract(document,'$.expert_step_attempt_id')=? AND id<>? AND json_extract(document,'$.status')<>'succeeded')")
                .bind(account).bind(attempt_id).bind(text(execution, "id")?).fetch_one(&mut **transaction).await?;
            if !pending {
                attempt["status"] = json!("running");
            }
        } else if matches!(execution["status"].as_str(), Some("failed" | "cancelled"))
            && attempt["status"] == "waiting_tool"
        {
            attempt["status"] = json!("failed");
            attempt["completed_at"] = execution["completed_at"].clone();
            attempt["check_summary"] = execution["error_message"].clone();
        }
        if attempt["result"]["workflow_execution_ids"].is_null() {
            attempt["result"]["workflow_execution_ids"] = json!([]);
        }
        let workflows = attempt["result"]["workflow_execution_ids"]
            .as_array_mut()
            .ok_or(Error::State("专家工作流结果需要数组"))?;
        if !workflows.contains(&execution["id"]) {
            workflows.push(execution["id"].clone());
        }
        if attempt["result"]["assets"].is_null() {
            attempt["result"]["assets"] = json!([]);
        }
        let assets = attempt["result"]["assets"]
            .as_array_mut()
            .ok_or(Error::State("专家资产结果需要数组"))?;
        for output in execution["outputs"]
            .as_array()
            .ok_or(Error::State("工作流输出记录缺失"))?
        {
            let reference = json!({ "asset_id": output["asset_id"], "asset_version_id": output["asset_version_id"] });
            if !assets.contains(&reference) {
                assets.push(reference);
            }
        }
        if attempt == original {
            return Ok(());
        }
        Self::save_expert_attempt_in(transaction, account, &mut attempt).await?;
        let step = task["steps"]
            .as_array_mut()
            .ok_or(Error::State("专家步骤格式无效"))?
            .iter_mut()
            .find(|step| step["latest_attempt_id"] == attempt_id);
        if let Some(step) =
            step.filter(|step| matches!(step["status"].as_str(), Some("running" | "waiting_tool")))
        {
            step["status"] = attempt["status"].clone();
            if task["current_step_id"] == attempt["step_id"]
                && matches!(task["status"].as_str(), Some("active" | "waiting_tool"))
            {
                task["status"] = json!(if attempt["status"] == "waiting_tool" {
                    "waiting_tool"
                } else {
                    "active"
                });
            }
            let run: String =
                sqlx::query_scalar("SELECT document FROM runs WHERE account_id=? AND id=?")
                    .bind(account)
                    .bind(text(execution, "run_id")?)
                    .fetch_one(&mut **transaction)
                    .await?;
            Self::save_expert_task_in(
                transaction,
                account,
                &serde_json::from_str(&run)?,
                &mut task,
            )
            .await?;
        }
        Ok(())
    }

    pub(crate) async fn execute_workflow(
        &self,
        account: &str,
        run: &Value,
        id: &str,
        tool: &RunTool,
        call: &ToolCall,
        workflow: &Value,
    ) -> Result<Value> {
        let run_id = text(run, "id")?;
        let session = text(run, "session_id")?;
        let saved: Option<String> = sqlx::query_scalar(
            "SELECT document FROM workflow_executions WHERE account_id=? AND id=? AND run_id=?",
        )
        .bind(account)
        .bind(id)
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        if saved.is_none() {
            let inputs = call
                .function
                .arguments
                .get("inputs")
                .ok_or(Error::Invalid("工作流输入缺失"))?;
            let transport = self
                .workflow_transport_inputs(account, workflow, inputs)
                .await?;
            let mut arguments = call.function.arguments.clone();
            arguments.insert("inputs".to_owned(), transport);
            let now = timestamp()?;
            let mut entries = Vec::new();
            let mut media = Vec::new();
            for (key, value) in inputs
                .as_object()
                .ok_or(Error::Invalid("工作流输入需要对象"))?
            {
                if value.is_object() {
                    let version_id = text(value, "asset_version_id")?;
                    let asset = self
                        .reference_asset(
                            account,
                            session,
                            text(value, "asset_id")?,
                            version_id,
                            &format!(
                                "workflow-input:{id}:{}",
                                crate::assets::digest(key.as_bytes())
                            ),
                        )
                        .await?;
                    entries.push(json!({ "key": key, "asset_id": asset.id, "asset_name": asset.name, "asset_version_id": value["asset_version_id"] }));
                    media.push((key, asset, version_id));
                } else {
                    entries.push(json!({ "key": key, "value": value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string()) }));
                }
            }
            let mut document = json!({
                "id": id, "run_id": run_id, "session_id": session, "task_id": "", "workflow_id": workflow["id"],
                "workflow": workflow, "operation_node_id": id, "status": "submitted", "submission_state": "submitting",
                "input_fields": workflow["input_fields"], "output_fields": workflow["output_fields"], "inputs": entries,
                "created_at": now, "connector": tool, "outputs": [],
            });
            if run["expert_id"].as_str().is_some_and(|id| !id.is_empty()) {
                let task = self.current_expert_task(account, run).await?;
                let step = task["steps"]
                    .as_array()
                    .ok_or(Error::State("专家步骤格式无效"))?
                    .iter()
                    .find(|step| step["step_id"] == task["current_step_id"])
                    .ok_or(Error::State("当前专家步骤缺失"))?;
                document["expert_task_id"] = task["id"].clone();
                document["expert_step_attempt_id"] = json!(text(step, "latest_attempt_id")?);
            }
            let mut transaction = self.pool.begin().await?;
            sqlx::query("INSERT INTO workflow_executions(id,account_id,session_id,run_id,created_at,document) VALUES(?,?,?,?,?,?)")
                .bind(id).bind(account).bind(session).bind(run_id).bind(&now).bind(document.to_string()).execute(&mut *transaction).await?;
            Self::record_expert_workflow_in(&mut transaction, account, &document).await?;
            let order: i64 = sqlx::query_scalar("SELECT coalesce(max(CAST(json_extract(document,'$.sort_order') AS INTEGER))+10,0) FROM flow_nodes WHERE account_id=? AND session_id=?")
                .bind(account).bind(session).fetch_one(&mut *transaction).await?;
            let node = json!({ "id": id, "type": "operation", "title": workflow["name"], "run_id": run_id, "outputs": [], "position": { "x": order * 30 + 300, "y": 0 }, "sort_order": order + media.len() as i64 * 10, "updated_at": now });
            sqlx::query("INSERT INTO flow_nodes(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)")
                .bind(id).bind(account).bind(session).bind(&now).bind(node.to_string()).execute(&mut *transaction).await?;
            for (index, (key, asset, version_id)) in media.iter().enumerate() {
                let source = format!("{id}-input-{index}");
                let version = asset
                    .versions
                    .iter()
                    .find(|version| version.id == *version_id)
                    .ok_or(Error::NotFound)?;
                let node = json!({ "id": source, "type": "asset", "title": asset.name, "asset_id": asset.id, "asset_version_id": version_id, "asset_version": version.version, "run_id": run_id, "position": { "x": order * 30, "y": index * 180 }, "sort_order": order + index as i64 * 10, "updated_at": now });
                sqlx::query("INSERT INTO flow_nodes(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)")
                    .bind(&source).bind(account).bind(session).bind(&now).bind(node.to_string()).execute(&mut *transaction).await?;
                let edge = json!({ "id": format!("{source}-edge"), "source": source, "target": id, "label": key, "source_output_key": "", "target_input_key": key });
                sqlx::query("INSERT INTO flow_edges(id,account_id,session_id,source,target,created_at,document) VALUES(?,?,?,?,?,?,?)")
                    .bind(format!("{source}-edge")).bind(account).bind(session).bind(source).bind(id).bind(&now).bind(edge.to_string()).execute(&mut *transaction).await?;
            }
            transaction.commit().await?;
            let response = self.call_mcp_tool(account, tool, &arguments).await?;
            let result = response
                .get("structuredContent")
                .ok_or(Error::ToolResultUnknown(
                    "工作流提交结果缺少结构化内容".to_owned(),
                ))?;
            let task_id = result["task_id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or(Error::ToolResultUnknown(
                    "工作流提交结果没有任务编号".to_owned(),
                ))?;
            let persisted: Result<()> = async {
                let mut transaction = self.pool.begin().await?;
                sqlx::query("UPDATE workflow_executions SET document=json_set(document,'$.task_id',?,'$.submission_state','submitted','$.task_status',?) WHERE account_id=? AND id=?")
                    .bind(task_id).bind(result["status"].as_str()).bind(account).bind(id).execute(&mut *transaction).await?;
                sqlx::query("UPDATE tool_calls SET status='waiting_workflow',updated_at=? WHERE account_id=? AND run_id=? AND id=? AND status='executing'")
                    .bind(timestamp()?).bind(account).bind(run_id).bind(id).execute(&mut *transaction).await?;
                sqlx::query("UPDATE runs SET document=json_set(document,'$.status','waiting_workflow') WHERE account_id=? AND id=? AND status='running'")
                    .bind(account).bind(run_id).execute(&mut *transaction).await?;
                transaction.commit().await?;
                Ok(())
            }.await;
            persisted.map_err(|error| {
                Error::ToolResultUnknown(format!("远程任务编号保存失败：{error}"))
            })?;
        }
        sqlx::query("UPDATE runs SET document=json_set(document,'$.status','waiting_workflow') WHERE account_id=? AND id=? AND status='running'")
            .bind(account).bind(run_id).execute(&self.pool).await?;
        loop {
            let document = self.refresh_workflow_execution(account, id).await?;
            match text(&document, "status")? {
                "succeeded" => {
                    return Ok(
                        json!({ "structuredContent": { "workflow_execution_id": id, "task_id": document["task_id"], "status": "succeeded", "inputs": document["inputs"], "outputs": document["outputs"] } }),
                    );
                }
                "failed" | "cancelled" => {
                    return Err(Error::Mcp(
                        document["error_message"]
                            .as_str()
                            .unwrap_or("远程工作流未完成")
                            .to_owned(),
                    ));
                }
                "submitted" => tokio::time::sleep(Duration::from_secs(1)).await,
                _ => return Err(Error::State("工作流执行状态无效")),
            }
        }
    }

    pub async fn reconcile_workflow_submission(&self, account: &str, id: &str) -> Result<Value> {
        Self::check_account(account)?;
        let saved: String = sqlx::query_scalar(
            "SELECT document FROM workflow_executions WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)?;
        let mut document: Value = serde_json::from_str(&saved)?;
        if document["submission_state"] != "result_unknown" || document["task_id"] != "" {
            return Err(Error::Invalid("当前工作流无需核对提交结果"));
        }
        let workflow_id: u64 = text(&document, "workflow_id")?
            .parse()
            .map_err(|_| Error::State("工作流标识无效"))?;
        let original: RunTool = serde_json::from_value(document["connector"].clone())?;
        let ToolTarget::Mcp { connector_id, .. } = &original.target else {
            return Err(Error::State("工作流 MCP 配置无效"));
        };
        let query = self.run_tools(account, text(&document, "run_id")?).await?.into_iter().find(|tool| {
            matches!(&tool.target, ToolTarget::Mcp { connector_id: candidate, name, .. } if candidate == connector_id && name == "list_tasks")
        }).ok_or(Error::State("MCP 任务列表工具缺失"))?;
        let response = self
            .call_mcp_tool(account, &query, &serde_json::Map::new())
            .await?;
        let tasks = response["structuredContent"]["tasks"]
            .as_array()
            .ok_or(Error::Invalid("任务列表缺少结构化内容"))?;
        let mut candidates = Vec::new();
        let mut descriptions = Vec::new();
        for task in tasks {
            let case = task["case_id"]
                .as_u64()
                .ok_or(Error::Invalid("任务工作流标识无效"))?;
            let task_id = text(task, "task_id")?;
            let status = text(task, "status")?;
            if task_id.is_empty()
                || ![
                    "pending",
                    "queued",
                    "running",
                    "succeeded",
                    "failed",
                    "cancelled",
                ]
                .contains(&status)
            {
                return Err(Error::Invalid("任务列表内容无效"));
            }
            if workflow_id == case {
                candidates.push(task.clone());
                descriptions.push(format!("{task_id}（{status}）"));
            }
        }
        document["reconciliation"] = json!({ "checked_at": timestamp()?, "tasks": candidates });
        let message = if descriptions.is_empty() {
            "工作流提交结果未知，禁止自动重复提交。当前凭据的最近任务中没有该工作流，需要继续核对。"
                .to_owned()
        } else {
            format!(
                "工作流提交结果未知，禁止自动重复提交。当前凭据中该工作流的最近任务：{}。请核对具体任务。",
                descriptions.join("；")
            )
        };
        document["error_message"] = json!(message);
        let saved: String = sqlx::query_scalar("UPDATE workflow_executions SET document=json_set(document,'$.reconciliation',json(?),'$.error_message',?) WHERE account_id=? AND id=? AND json_extract(document,'$.submission_state')='result_unknown' AND json_extract(document,'$.task_id')='' RETURNING document")
            .bind(document["reconciliation"].to_string()).bind(message).bind(account).bind(id).fetch_optional(&self.pool).await?.ok_or(Error::Invalid("工作流提交状态已经更新"))?;
        Ok(serde_json::from_str(&saved)?)
    }

    pub async fn refresh_workflow_execution(&self, account: &str, id: &str) -> Result<Value> {
        Self::check_account(account)?;
        let saved: String = sqlx::query_scalar(
            "SELECT document FROM workflow_executions WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)?;
        let mut document: Value = serde_json::from_str(&saved)?;
        if document["status"] != "submitted" {
            return Ok(document);
        }
        let task = text(&document, "task_id")?.to_owned();
        if task.is_empty() {
            return Err(Error::ToolResultUnknown(
                "工作流提交尚未取得任务编号".to_owned(),
            ));
        }
        let original: RunTool = serde_json::from_value(document["connector"].clone())?;
        let ToolTarget::Mcp { connector_id, .. } = &original.target else {
            return Err(Error::State("工作流 MCP 配置无效"));
        };
        let query = self.run_tools(account, text(&document, "run_id")?).await?.into_iter().find(|tool| {
            matches!(&tool.target, ToolTarget::Mcp { connector_id: candidate, name, .. } if candidate == connector_id && name == "get_task")
        }).ok_or(Error::State("MCP 任务查询工具缺失"))?;
        let response = self
            .call_mcp_tool(
                account,
                &query,
                &serde_json::Map::from_iter([("task_id".to_owned(), json!(task))]),
            )
            .await?;
        let result = response
            .get("structuredContent")
            .ok_or(Error::Invalid("任务查询缺少结构化内容"))?;
        if result["task_id"] != task
            || result["case_id"]
                .as_u64()
                .map(|id| id.to_string())
                .as_deref()
                != document["workflow_id"].as_str()
        {
            return Err(Error::Invalid("远程任务与工作流记录不一致"));
        }
        let status = text(result, "status")?;
        document["task_status"] = json!(status);
        match status {
            "pending" | "queued" | "running" => {}
            "failed" | "cancelled" => {
                document["status"] = json!(status);
                document["error_message"] =
                    json!(result["error"].as_str().unwrap_or("远程工作流已结束"));
                document["completed_at"] = json!(timestamp()?);
            }
            "succeeded" => {
                let mut outputs = document["outputs"]
                    .as_array()
                    .cloned()
                    .ok_or(Error::State("工作流输出记录缺失"))?;
                for (index, output) in result["outputs"]
                    .as_array()
                    .ok_or(Error::Invalid("任务结果列表缺失"))?
                    .iter()
                    .enumerate()
                {
                    let uri = text(output, "uri")?;
                    let expected = format!("pixoma://task/{task}/output/{index}");
                    if uri != expected {
                        return Err(Error::Invalid("任务输出地址不匹配"));
                    }
                    if let Some(saved) = outputs.get(index) {
                        if saved["uri"] != uri || saved["key"] != output["key"] {
                            return Err(Error::Invalid("工作流输出记录发生变化"));
                        }
                        self.asset_path(
                            account,
                            text(saved, "asset_id")?,
                            text(saved, "asset_version_id")?,
                        )
                        .await?;
                        continue;
                    }
                    let resources = self.read_mcp_resource(account, &original, uri).await?;
                    if resources.len() != 1 {
                        return Err(Error::Invalid("工作流输出需要单个资源"));
                    }
                    let bytes = match &resources[0] {
                        rmcp::model::ResourceContents::BlobResourceContents {
                            uri: actual,
                            blob,
                            ..
                        } if actual == uri => {
                            if blob.len() > 358_000_000 {
                                return Err(Error::Invalid("工作流结果超过大小限制"));
                            }
                            STANDARD
                                .decode(blob)
                                .map_err(|_| Error::Invalid("工作流结果 Base64 无效"))?
                        }
                        rmcp::model::ResourceContents::TextResourceContents {
                            uri: actual,
                            text,
                            ..
                        } if actual == uri => text.as_bytes().to_vec(),
                        _ => return Err(Error::Invalid("工作流结果资源无效")),
                    };
                    let (media_type, textual) = match &resources[0] {
                        rmcp::model::ResourceContents::BlobResourceContents {
                            mime_type, ..
                        } => (mime_type.as_deref(), false),
                        rmcp::model::ResourceContents::TextResourceContents {
                            mime_type, ..
                        } => (mime_type.as_deref(), true),
                        _ => return Err(Error::Invalid("工作流结果资源类型不受支持")),
                    };
                    let extension = workflow_output_extension(
                        &bytes,
                        media_type.or_else(|| output["mime"].as_str()),
                        textual,
                    )?;
                    let name = format!("{}.{}", text(output, "key")?, extension);
                    let asset = self
                        .create_asset_bytes(
                            account,
                            Some(text(&document, "session_id")?),
                            &name,
                            &bytes,
                            "workflow",
                            &format!("workflow:{id}:{index}"),
                        )
                        .await?;
                    let version = asset
                        .versions
                        .last()
                        .ok_or(Error::State("工作流资产版本缺失"))?;
                    outputs.push(json!({ "key": output["key"], "type": asset.kind, "name": asset.name, "asset_id": asset.id, "asset_version_id": version.id, "uri": uri }));
                    let mut transaction = self.pool.begin().await?;
                    let changed = sqlx::query("UPDATE workflow_executions SET document=json_set(document,'$.outputs',json(?)) WHERE account_id=? AND id=? AND json_extract(document,'$.status')='submitted' AND json_array_length(document,'$.outputs')<?")
                        .bind(serde_json::to_string(&outputs)?).bind(account).bind(id).bind(outputs.len() as i64).execute(&mut *transaction).await?;
                    if changed.rows_affected() == 1 {
                        sqlx::query("UPDATE flow_nodes SET document=json_set(document,'$.outputs',json(?),'$.updated_at',?) WHERE account_id=? AND id=?")
                            .bind(serde_json::to_string(&outputs)?).bind(timestamp()?).bind(account).bind(text(&document, "operation_node_id")?).execute(&mut *transaction).await?;
                        document["outputs"] = json!(&outputs);
                        Self::record_expert_workflow_in(&mut transaction, account, &document)
                            .await?;
                    }
                    transaction.commit().await?;
                }
                document["outputs"] = json!(outputs);
                document["status"] = json!("succeeded");
                document["completed_at"] = json!(timestamp()?);
            }
            _ => return Err(Error::Invalid("远程任务状态无效")),
        }
        let mut transaction = self.pool.begin().await?;
        sqlx::query("UPDATE workflow_executions SET document=json_set(document,'$.status',?,'$.task_status',?,'$.completed_at',?,'$.error_message',?) WHERE account_id=? AND id=? AND json_extract(document,'$.status')='submitted'")
            .bind(text(&document, "status")?).bind(text(&document, "task_status")?).bind(document["completed_at"].as_str()).bind(document["error_message"].as_str()).bind(account).bind(id).execute(&mut *transaction).await?;
        Self::record_expert_workflow_in(&mut transaction, account, &document).await?;
        transaction.commit().await?;
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn workflow_outputs_keep_their_declared_format_and_binary_bytes() -> Result<()> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("workflow-output-format-")
            .tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let session = store.create_session("output-format", "outputs").await?;
        assert!(workflow_output_extension(b"content", Some("invalid"), false).is_err());
        assert!(workflow_output_extension(b"content", Some("image/*"), false).is_err());
        for (expected_extension, media_type, textual, bytes, expected_kind, expected_mime) in [
            (
                "json",
                Some("application/json; charset=utf-8"),
                false,
                serde_json::to_vec(&json!({ "width": 12, "height": 9 }))?,
                "data",
                "application/json",
            ),
            (
                "bin",
                None,
                false,
                vec![0, 255, 17, 0, 3, 1],
                "file",
                "application/octet-stream",
            ),
            (
                "txt",
                Some("text/plain; charset=utf-8"),
                true,
                "计算结果".as_bytes().to_vec(),
                "document",
                "text/plain",
            ),
        ] {
            let source = directory
                .path()
                .join(format!("source.{expected_extension}"));
            std::fs::write(&source, &bytes)?;
            let bytes = std::fs::read(source)?;
            let extension = workflow_output_extension(&bytes, media_type, textual)?;
            let asset = store
                .create_asset_bytes(
                    "output-format",
                    Some(&session.id),
                    &format!("result.{extension}"),
                    &bytes,
                    "workflow",
                    expected_extension,
                )
                .await?;
            assert_eq!(asset.kind, expected_kind);
            assert_eq!(asset.versions[0].mime_type, expected_mime);
            assert_eq!(
                store
                    .read_asset("output-format", &asset.id, &asset.versions[0].id)
                    .await?,
                bytes
            );
        }
        store.close().await;
        Ok(())
    }
}
