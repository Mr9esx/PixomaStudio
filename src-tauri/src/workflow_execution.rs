use crate::{Error, Result, Store, error::timestamp, local_api::text};
use rig::message::ToolCall;
use serde_json::{Value, json};
use std::time::Duration;

impl Store {
    async fn record_expert_workflow_in(transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>, execution: &Value) -> Result<()> {
        let Some(attempt_id) = execution["expert_step_attempt_id"].as_str() else { return Ok(()); };
        let mut attempt = Self::expert_attempt_in(transaction, attempt_id).await?;
        if attempt["expert_task_id"] != execution["expert_task_id"] { return Err(Error::State("工作流与专家处理记录不一致")); }
        let original = attempt.clone();
        let mut task = Self::expert_task_in(transaction, text(&attempt, "expert_task_id")?).await?;
        if execution["submission_state"] == "submitting" {
            if !matches!(attempt["status"].as_str(), Some("running" | "waiting_tool"))
                || !matches!(task["status"].as_str(), Some("active" | "waiting_tool"))
                || task["current_step_id"] != attempt["step_id"]
                || !task["steps"].as_array().ok_or(Error::State("专家步骤格式无效"))?.iter().any(|step| step["latest_attempt_id"] == attempt_id && step["step_id"] == attempt["step_id"] && matches!(step["status"].as_str(), Some("running" | "waiting_tool"))) {
                return Err(Error::Invalid("当前专家步骤尚未开始或已经更改"));
            }
            attempt["status"] = json!("waiting_tool");
        } else if execution["status"] == "succeeded" && attempt["status"] == "waiting_tool" {
            let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workflow_executions WHERE json_extract(document,'$.expert_step_attempt_id')=? AND id<>? AND json_extract(document,'$.status')<>'succeeded')")
                .bind(attempt_id).bind(text(execution, "id")?).fetch_one(&mut **transaction).await?;
            if !pending { attempt["status"] = json!("running"); }
        } else if matches!(execution["status"].as_str(), Some("failed" | "cancelled" | "result_unknown")) && attempt["status"] == "waiting_tool" {
            attempt["status"] = json!("failed");
            attempt["completed_at"] = execution["completed_at"].clone();
            attempt["check_summary"] = execution["error_message"].clone();
        }
        if attempt["result"]["workflow_execution_ids"].is_null() { attempt["result"]["workflow_execution_ids"] = json!([]); }
        let workflows = attempt["result"]["workflow_execution_ids"].as_array_mut().ok_or(Error::State("专家工作流结果需要数组"))?;
        if !workflows.contains(&execution["id"]) { workflows.push(execution["id"].clone()); }
        if attempt["result"]["assets"].is_null() { attempt["result"]["assets"] = json!([]); }
        let assets = attempt["result"]["assets"].as_array_mut().ok_or(Error::State("专家资产结果需要数组"))?;
        for output in execution["outputs"].as_array().ok_or(Error::State("工作流输出记录缺失"))? {
            let reference = json!({ "asset_id": output["asset_id"], "asset_version_id": output["asset_version_id"] });
            if !assets.contains(&reference) { assets.push(reference); }
        }
        if attempt == original { return Ok(()); }
        Self::save_expert_attempt_in(transaction, &mut attempt).await?;
        if let Some(step) = task["steps"].as_array_mut().ok_or(Error::State("专家步骤格式无效"))?.iter_mut()
            .find(|step| step["latest_attempt_id"] == attempt_id && matches!(step["status"].as_str(), Some("running" | "waiting_tool"))) {
            step["status"] = attempt["status"].clone();
            if task["current_step_id"] == attempt["step_id"] && matches!(task["status"].as_str(), Some("active" | "waiting_tool")) {
                task["status"] = json!(if attempt["status"] == "waiting_tool" { "waiting_tool" } else { "active" });
            }
            let run: String = sqlx::query_scalar("SELECT document FROM runs WHERE id=?").bind(text(execution, "run_id")?).fetch_one(&mut **transaction).await?;
            Self::save_expert_task_in(transaction, &serde_json::from_str(&run)?, &mut task).await?;
        }
        Ok(())
    }

    pub(crate) async fn execute_workflow(&self, run: &Value, id: &str, call: &ToolCall, workflow: &Value) -> Result<Value> {
        let run_id = text(run, "id")?;
        let session = text(run, "session_id")?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workflow_executions WHERE id=? AND run_id=?)")
            .bind(id).bind(run_id).fetch_one(&self.pool).await?;
        if !exists {
            let inputs = call.function.arguments.get("inputs").ok_or(Error::Invalid("工作流输入缺失"))?;
            self.workflow_transport_inputs(workflow, inputs).await?;
            let now = timestamp()?;
            let mut entries = Vec::new();
            for (key, value) in inputs.as_object().ok_or(Error::Invalid("工作流输入需要对象"))? {
                if value.is_object() || value.is_array() {
                    for (index,reference) in value.as_array().map(Vec::as_slice).unwrap_or_else(||std::slice::from_ref(value)).iter().enumerate() {
                        let version_id = text(reference, "asset_version_id")?;
                        let asset = self.reference_asset(session, text(reference, "asset_id")?, version_id, &format!("workflow-input:{id}:{}:{index}", crate::assets::digest(key.as_bytes()))).await?;
                        entries.push(json!({ "key": key, "index":index, "asset_id": asset.id, "asset_name": asset.name, "asset_version_id": version_id }));
                    }
                } else { entries.push(json!({ "key": key, "value": value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string()) })); }
            }
            let mut document = json!({ "id": id, "run_id": run_id, "session_id": session, "execution_id": null,
                "request_id": format!("workflow-tool:{id}"), "workflow_id": workflow["id"], "workflow_version_id": workflow["version_id"],
                "workflow": workflow, "status": "submitted", "submission_state": "submitting",
                "input_fields": workflow["input_fields"], "output_fields": workflow["output_fields"], "inputs": entries, "input_values": inputs,
                "created_at": now, "outputs": [] });
            if run["expert_id"].as_str().is_some_and(|id| !id.is_empty()) {
                let task = self.current_expert_task(run).await?;
                let step = task["steps"].as_array().ok_or(Error::State("专家步骤格式无效"))?.iter().find(|step| step["step_id"] == task["current_step_id"]).ok_or(Error::State("当前专家步骤缺失"))?;
                document["expert_task_id"] = task["id"].clone();
                document["expert_step_attempt_id"] = json!(text(step, "latest_attempt_id")?);
            }
            let mut transaction = self.pool.begin().await?;
            let changed = sqlx::query("UPDATE tool_calls SET status='waiting_workflow',updated_at=? WHERE run_id=? AND id=? AND status='executing'")
                .bind(&now).bind(run_id).bind(id).execute(&mut *transaction).await?;
            if changed.rows_affected() != 1 { return Err(Error::State("工作流工具调用状态已更改")); }
            let changed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','waiting_workflow') WHERE id=? AND status='running'")
                .bind(run_id).execute(&mut *transaction).await?;
            if changed.rows_affected() != 1 { return Err(Error::Cancelled); }
            sqlx::query("INSERT INTO workflow_executions(id,session_id,run_id,created_at,document) VALUES(?,?,?,?,?)")
                .bind(id).bind(session).bind(run_id).bind(&now).bind(document.to_string()).execute(&mut *transaction).await?;
            Self::record_expert_workflow_in(&mut transaction, &document).await?;
            transaction.commit().await?;
        }
        self.reconcile_workflow_submission(id).await?;
        loop {
            let document = self.refresh_workflow_execution(id).await?;
            match text(&document, "status")? {
                "succeeded" => return Ok(json!({ "workflow_execution_id": id, "execution_id": document["execution_id"], "status": "succeeded", "inputs": document["inputs"], "outputs": document["outputs"] })),
                "failed" | "cancelled" | "result_unknown" => return Err(Error::Validation(document["error_message"].as_str().unwrap_or("工作流未完成").to_owned())),
                "submitted" => tokio::time::sleep(Duration::from_secs(1)).await,
                _ => return Err(Error::State("工作流执行状态无效")),
            }
        }
    }

    pub async fn reconcile_workflow_submission(&self, id: &str) -> Result<Value> {
        let saved: String = sqlx::query_scalar("SELECT document FROM workflow_executions WHERE id=?").bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let mut document: Value = serde_json::from_str(&saved)?;
        if document["execution_id"].as_str().is_some() || document["status"] != "submitted" { return Ok(document); }
        let run = self.get_run(text(&document, "run_id")?).await?;
        let request_id = text(&document, "request_id")?;
        let existing: Option<String> = sqlx::query_scalar("SELECT id FROM local_executions WHERE request_id=? UNION ALL SELECT id FROM online_executions WHERE request_id=?").bind(request_id).bind(request_id).fetch_optional(&self.pool).await?;
        let execution = match existing {
            Some(id) => self.get_local_execution(&id).await?,
            None if matches!(run["status"].as_str(), Some("queued" | "running" | "waiting_workflow")) => self.create_local_execution(json!({
                "request_id": request_id, "workflow_id": document["workflow_id"], "version_id": document["workflow_version_id"],
                "session_id": document["session_id"], "inputs": document["input_values"],
            })).await?,
            None => return Err(Error::Cancelled),
        };
        if execution["workflow_id"] != document["workflow_id"] || execution["workflow_version_id"] != document["workflow_version_id"] || execution["session_id"] != document["session_id"] {
            return Err(Error::State("工作流执行与提交记录不一致"));
        }
        document["execution_id"] = execution["id"].clone();
        document["submission_state"] = json!("submitted");
        document["execution_status"] = execution["status"].clone();
        sqlx::query("UPDATE workflow_executions SET document=? WHERE id=? AND json_extract(document,'$.status')='submitted' AND json_extract(document,'$.execution_id') IS NULL")
            .bind(document.to_string()).bind(id).execute(&self.pool).await?;
        Ok(document)
    }

    pub async fn refresh_workflow_execution(&self, id: &str) -> Result<Value> {
        let mut document = self.reconcile_workflow_submission(id).await?;
        if document["status"] != "submitted" { return Ok(document); }
        let execution = self.get_local_execution(text(&document, "execution_id")?).await?;
        document["execution_status"] = execution["status"].clone();
        let mut outputs = Vec::new();
        for output in execution["outputs"].as_array().ok_or(Error::State("工作流执行输出缺失"))? {
            let asset = self.get_asset(text(output, "asset_id")?).await?;
            let version = asset.versions.iter().find(|version| Some(version.id.as_str()) == output["asset_version_id"].as_str()).ok_or(Error::NotFound)?;
            self.asset_path(&asset.id, &version.id).await?;
            outputs.push(json!({ "key": output["field_key"], "index": output["index"], "type": asset.kind, "name": asset.name,
                "asset_id": asset.id, "asset_version_id": version.id, "media_type": version.mime_type }));
        }
        document["outputs"] = json!(outputs);
        if crate::executions::terminal(text(&execution, "status")?) {
            document["status"] = execution["status"].clone();
            document["error_message"] = execution["error"].clone();
            document["completed_at"] = execution["updated_at"].clone();
        }
        let mut transaction = self.pool.begin().await?;
        sqlx::query("UPDATE workflow_executions SET document=? WHERE id=? AND json_extract(document,'$.status')='submitted'")
            .bind(document.to_string()).bind(id).execute(&mut *transaction).await?;
        Self::record_expert_workflow_in(&mut transaction, &document).await?;
        transaction.commit().await?;
        Ok(document)
    }
}
