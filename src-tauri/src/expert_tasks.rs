use crate::{
    Error, Result, Store,
    error::timestamp,
    experts::ExpertContent,
    local_api::{optional_text, text},
    tools::{RunTool, ToolTarget},
};
use rig::{completion::ToolDefinition, message::ToolName};
use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) fn expert_tools() -> Result<Vec<RunTool>> {
    let result = json!({ "type": "object", "additionalProperties": false, "properties": {
        "message_ids": { "type": "array", "items": { "type": "string" } },
        "assets": { "type": "array", "items": { "type": "object", "additionalProperties": false,
            "properties": { "asset_id": { "type": "string" }, "asset_version_id": { "type": "string" } }, "required": ["asset_id", "asset_version_id"] } },
        "workflow_execution_ids": { "type": "array", "items": { "type": "string" } }, "summary": { "type": "string" },
    }});
    let field = json!({ "type": "object", "additionalProperties": false, "properties": {
        "key": { "type": "string", "minLength": 1 }, "label": { "type": "string", "minLength": 1 },
        "type": { "enum": ["text", "textarea", "number", "select", "asset", "assets", "boolean"] },
        "required": { "type": "boolean" }, "options": { "type": "array", "items": { "type": "string" } },
    }, "required": ["key", "label", "type", "required"] });
    [
        ("update_expert_task", "保存或读取当前专家任务。create_task 创建或读取进行中的任务；read_task 返回实际进度、处理记录和结果标识；start_step 使用本轮真实用户消息的 authorization_message_id 记录开始授权与 inputs；update_step 使用实际 attempt_id、status 和 result 保存进度；skip_step 记录本轮用户授权的跳过决定；update_task 使用 status 保存任务状态，通过 check_summary 填写任务说明。步骤检查说明也使用 check_summary。", json!({
            "type": "object", "additionalProperties": false, "properties": {
                "action": { "enum": ["create_task", "read_task", "start_step", "update_step", "skip_step", "update_task"] },
                "goal": { "type": "string" }, "step_id": { "type": "string" }, "attempt_id": { "type": "string" },
                "status": {
                    "enum": ["active", "running", "waiting_user", "waiting_tool", "paused", "completed", "cancelled", "failed"],
                    "description": "update_step 使用 running、waiting_user、waiting_tool、completed 或 failed；update_task 使用 active、waiting_user、waiting_tool、paused、completed、cancelled 或 failed。",
                }, "inputs": { "type": "object" }, "authorization_message_id": { "type": "string" },
                "result": result, "check_summary": { "type": "string", "description": "步骤检查或任务完成的说明，用于 update_step、skip_step 和 update_task。" },
            }, "required": ["action"],
        })),
        ("request_expert_confirmation", "为当前专家任务展示步骤输入或结果确认卡片。step_entry 收集输入并等待开始决定；result_review 展示实际 message_ids、资产版本或工作流结果，等待用户确认或修改。创建卡片后停止本轮，等待用户选择。", json!({
            "type": "object", "additionalProperties": false, "properties": {
                "goal": { "type": "string" }, "step_id": { "type": "string", "minLength": 1 }, "attempt_id": { "type": "string" },
                "card": { "type": "object", "additionalProperties": false, "properties": {
                    "mode": { "enum": ["step_entry", "result_review"] }, "reason": { "type": "string", "minLength": 1, "maxLength": 2000 },
                    "fields": { "type": "array", "items": field }, "suggested_inputs": { "type": "object" }, "results": result,
                }, "required": ["mode", "reason"] },
            }, "required": ["step_id", "card"],
        })),
    ].into_iter().map(|(name, description, parameters)| Ok(RunTool {
        definition: ToolDefinition::new(ToolName::new(name).map_err(|_| Error::State("专家工具名称无效"))?, description, parameters),
        target: ToolTarget::Expert, requires_approval: false, read_only: false,
    })).collect()
}

impl Store {
    pub(crate) async fn expert_task_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        id: &str,
    ) -> Result<Value> {
        let document: String =
            sqlx::query_scalar("SELECT document FROM expert_tasks WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_optional(&mut **transaction)
                .await?
                .ok_or(Error::NotFound)?;
        Ok(serde_json::from_str(&document)?)
    }

    pub(crate) async fn expert_attempt_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        id: &str,
    ) -> Result<Value> {
        let document: String = sqlx::query_scalar(
            "SELECT document FROM expert_step_attempts WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or(Error::NotFound)?;
        Ok(serde_json::from_str(&document)?)
    }

    pub(crate) async fn save_expert_task_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        run: &Value,
        task: &mut Value,
    ) -> Result<()> {
        let revision = task["revision"]
            .as_u64()
            .ok_or(Error::State("专家任务缺少版本"))?;
        task["revision"] = json!(
            revision
                .checked_add(1)
                .ok_or(Error::State("专家任务版本超过范围"))?
        );
        task["updated_at"] = json!(timestamp()?);
        let changed = sqlx::query("UPDATE expert_tasks SET document=? WHERE account_id=? AND id=? AND json_extract(document,'$.revision')=?")
            .bind(task.to_string()).bind(account).bind(text(task, "id")?).bind(revision as i64).execute(&mut **transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Invalid("专家任务已更新，请重新读取"));
        }
        Self::append_event_in(
            transaction,
            account,
            text(run, "session_id")?,
            text(run, "id")?,
            json!({
                "type": "CUSTOM", "name": "EXPERT_TASK_UPDATED", "value": { "task": task },
            }),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn save_expert_attempt_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        attempt: &mut Value,
    ) -> Result<()> {
        let revision = attempt["revision"]
            .as_u64()
            .ok_or(Error::State("专家处理记录缺少版本"))?;
        attempt["revision"] = json!(
            revision
                .checked_add(1)
                .ok_or(Error::State("专家处理记录版本超过范围"))?
        );
        attempt["updated_at"] = json!(timestamp()?);
        let changed = sqlx::query("UPDATE expert_step_attempts SET document=? WHERE account_id=? AND id=? AND json_extract(document,'$.revision')=?")
            .bind(attempt.to_string()).bind(account).bind(text(attempt, "id")?).bind(revision as i64).execute(&mut **transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Invalid("专家处理记录已更新，请重新读取"));
        }
        Ok(())
    }

    pub(crate) async fn start_expert_attempt_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        run: &Value,
        task: &mut Value,
        step_id: &str,
        inputs: &Value,
    ) -> Result<Value> {
        if !inputs.is_object() {
            return Err(Error::Invalid("步骤输入需要 JSON 对象"));
        }
        let creation_key = format!("{}:{step_id}", text(run, "id")?);
        let previous: Option<String> = sqlx::query_scalar(
            "SELECT document FROM expert_step_attempts WHERE account_id=? AND creation_key=?",
        )
        .bind(account)
        .bind(&creation_key)
        .fetch_optional(&mut **transaction)
        .await?;
        if let Some(previous) = previous {
            let previous: Value = serde_json::from_str(&previous)?;
            if previous["inputs"] != *inputs {
                return Err(Error::Invalid("本轮步骤已经使用其他输入开始"));
            }
            return Ok(previous);
        }
        let now = timestamp()?;
        let id = Uuid::new_v4().to_string();
        let attempt = json!({ "id": id, "expert_task_id": task["id"], "step_id": step_id, "status": "running", "inputs": inputs,
            "start_message_id": run["trigger_message_id"], "run_id": run["id"], "result": {}, "check_summary": "", "revision": 1,
            "started_at": now, "updated_at": now });
        sqlx::query("INSERT INTO expert_step_attempts(id,account_id,session_id,expert_task_id,step_id,creation_key,created_at,document) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&id).bind(account).bind(text(run, "session_id")?).bind(text(task, "id")?).bind(step_id).bind(creation_key).bind(now).bind(attempt.to_string()).execute(&mut **transaction).await?;
        let step = task["steps"]
            .as_array_mut()
            .ok_or(Error::State("专家任务步骤格式无效"))?
            .iter_mut()
            .find(|step| step["step_id"] == step_id)
            .ok_or(Error::Invalid("专家步骤不存在"))?;
        step["status"] = json!("running");
        step["latest_attempt_id"] = json!(id);
        task["status"] = json!("active");
        task["current_step_id"] = json!(step_id);
        Ok(attempt)
    }

    pub(crate) async fn current_expert_task(&self, account: &str, run: &Value) -> Result<Value> {
        let session = self.get_session(account, text(run, "session_id")?).await?;
        let id = session
            .current_expert_task_id
            .ok_or(Error::Invalid("请先创建专家任务"))?;
        let mut transaction = self.pool.begin().await?;
        let task = Self::expert_task_in(&mut transaction, account, &id).await?;
        transaction.commit().await?;
        Ok(task)
    }

    pub(crate) async fn validate_expert_result_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        session: &str,
        result: &Value,
        completed: bool,
    ) -> Result<()> {
        if !result.is_object() {
            return Err(Error::Invalid("步骤结果需要 JSON 对象"));
        }
        let mut references = 0;
        for id in result
            .get("message_ids")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = id.as_str().ok_or(Error::Invalid("结果消息标识无效"))?;
            let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM messages WHERE account_id=? AND session_id=? AND id=?)")
                .bind(account).bind(session).bind(id).fetch_one(&mut **transaction).await?;
            if !exists {
                return Err(Error::Invalid("结果消息不属于当前会话"));
            }
            references += 1;
        }
        for asset in result
            .get("assets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_usages WHERE account_id=? AND session_id=? AND asset_id=? AND asset_version_id=?)")
                .bind(account).bind(session).bind(text(asset, "asset_id")?).bind(text(asset, "asset_version_id")?).fetch_one(&mut **transaction).await?;
            if !exists {
                return Err(Error::Invalid("结果资产版本不属于当前会话"));
            }
            references += 1;
        }
        for id in result
            .get("workflow_execution_ids")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = id.as_str().ok_or(Error::Invalid("结果工作流标识无效"))?;
            let document: Option<String> = sqlx::query_scalar("SELECT document FROM workflow_executions WHERE account_id=? AND session_id=? AND id=?")
                .bind(account).bind(session).bind(id).fetch_optional(&mut **transaction).await?;
            let document: Value = serde_json::from_str(&document.ok_or(Error::NotFound)?)?;
            if completed && document["status"] != "succeeded" {
                return Err(Error::Invalid("结果工作流尚未完成"));
            }
            references += 1;
        }
        if completed && references == 0 {
            return Err(Error::Invalid("完成步骤需要实际消息、资产或工作流结果"));
        }
        Ok(())
    }

    pub(crate) async fn validate_expert_attempt_workflows_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        attempt_id: &str,
    ) -> Result<()> {
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workflow_executions WHERE account_id=? AND json_extract(document,'$.expert_step_attempt_id')=? AND coalesce(json_extract(document,'$.status'),'')<>'succeeded')")
            .bind(account).bind(attempt_id).fetch_one(&mut **transaction).await?;
        if pending {
            return Err(Error::Invalid("当前步骤仍有尚未完成的工作流"));
        }
        Ok(())
    }

    async fn create_expert_task(&self, account: &str, run: &Value, input: &Value) -> Result<Value> {
        let session = self.get_session(account, text(run, "session_id")?).await?;
        let saved: String =
            sqlx::query_scalar("SELECT expert_snapshot FROM sessions WHERE account_id=? AND id=?")
                .bind(account)
                .bind(&session.id)
                .fetch_one(&self.pool)
                .await?;
        let content: ExpertContent = serde_json::from_str(&saved)?;
        if content.sop.is_empty() {
            return Err(Error::Invalid("当前专家没有定义步骤"));
        }
        let mut transaction = self.pool.begin().await?;
        if let Some(id) = session.current_expert_task_id {
            let task = Self::expert_task_in(&mut transaction, account, &id).await?;
            if !["completed", "cancelled", "failed"].contains(&text(&task, "status")?) {
                transaction.commit().await?;
                return Ok(task);
            }
        }
        let previous: Option<String> = sqlx::query_scalar("SELECT document FROM expert_tasks WHERE account_id=? AND json_extract(document,'$.creation_key')=?")
            .bind(account).bind(text(run, "id")?).fetch_optional(&mut *transaction).await?;
        if let Some(previous) = previous {
            transaction.commit().await?;
            return Ok(serde_json::from_str(&previous)?);
        }
        let goal = text(input, "goal")?.trim();
        if goal.is_empty() {
            return Err(Error::Invalid("专家任务目标不能为空"));
        }
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        let steps: Vec<Value> = content
            .sop
            .iter()
            .map(|step| json!({ "step_id": step.id, "title": step.title, "status": "pending" }))
            .collect();
        let task = json!({ "id": id, "session_id": session.id, "goal": goal, "status": "active", "current_step_id": "",
            "steps": steps, "summary": "", "revision": 1, "creation_key": run["id"], "created_at": now, "updated_at": now });
        sqlx::query("INSERT INTO expert_tasks(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)")
            .bind(&id).bind(account).bind(&session.id).bind(&now).bind(task.to_string()).execute(&mut *transaction).await?;
        sqlx::query("UPDATE sessions SET current_expert_task_id=? WHERE account_id=? AND id=?")
            .bind(&id)
            .bind(account)
            .bind(&session.id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE runs SET document=json_set(document,'$.expert_task_id',?) WHERE account_id=? AND id=?")
            .bind(&id).bind(account).bind(text(run, "id")?).execute(&mut *transaction).await?;
        Self::append_event_in(
            &mut transaction,
            account,
            &session.id,
            text(run, "id")?,
            json!({ "type": "CUSTOM", "name": "EXPERT_TASK_UPDATED", "value": { "task": task } }),
        )
        .await?;
        transaction.commit().await?;
        Ok(task)
    }

    pub(crate) async fn call_expert_control(
        &self,
        account: &str,
        run_id: &str,
        call_id: &str,
        name: &ToolName,
        input: &Value,
    ) -> Result<Value> {
        let run = self.get_run(account, run_id).await?;
        if run["expert_id"].as_str().is_none_or(str::is_empty) {
            return Err(Error::Invalid("当前运行没有绑定专家"));
        }
        if name.as_str() == "request_expert_confirmation" {
            return self.create_expert_card(account, &run, call_id, input).await;
        }
        if name.as_str() != "update_expert_task" {
            return Err(Error::Invalid("专家工具未注册"));
        }
        let action = text(input, "action")?;
        if action == "create_task" {
            return self.create_expert_task(account, &run, input).await;
        }
        let task = self.current_expert_task(account, &run).await?;
        if action == "read_task" {
            let attempts: Vec<String> = sqlx::query_scalar("SELECT document FROM expert_step_attempts WHERE account_id=? AND expert_task_id=? ORDER BY created_at,id")
                .bind(account).bind(text(&task, "id")?).fetch_all(&self.pool).await?;
            let mut result = task;
            result["attempts"] = json!(
                attempts
                    .iter()
                    .map(|attempt| serde_json::from_str::<Value>(attempt))
                    .collect::<std::result::Result<Vec<_>, _>>()?
            );
            let messages: Vec<String> = sqlx::query_scalar("SELECT id FROM messages WHERE account_id=? AND session_id=? AND json_extract(document,'$.role')='assistant' ORDER BY created_at DESC,id DESC LIMIT 50")
                .bind(account).bind(text(&run, "session_id")?).fetch_all(&self.pool).await?;
            result["available_results"] = json!({ "message_ids": messages });
            return Ok(result);
        }
        self.update_expert_progress(account, &run, task, input)
            .await
    }

    async fn update_expert_progress(
        &self,
        account: &str,
        run: &Value,
        mut task: Value,
        input: &Value,
    ) -> Result<Value> {
        let action = text(input, "action")?;
        let step_id = optional_text(input, "step_id", "")?;
        let index = task["steps"]
            .as_array()
            .ok_or(Error::State("专家步骤格式无效"))?
            .iter()
            .position(|step| step["step_id"] == step_id);
        let mut transaction = self.pool.begin().await?;
        match action {
            "start_step" => {
                if index.is_none() || input["authorization_message_id"] != run["trigger_message_id"]
                {
                    return Err(Error::Invalid("开始步骤需要本轮用户消息授权"));
                }
                Self::validate_expert_start_in(&mut transaction, account, run, step_id).await?;
                if run["trigger_kind"] == "card_response" {
                    transaction.commit().await?;
                    return Ok(task);
                }
                let inputs = input.get("inputs").cloned().unwrap_or_else(|| json!({}));
                Self::start_expert_attempt_in(
                    &mut transaction,
                    account,
                    run,
                    &mut task,
                    step_id,
                    &inputs,
                )
                .await?;
            }
            "skip_step" => {
                let index = index.ok_or(Error::Invalid("专家步骤不存在"))?;
                if run["trigger_kind"] == "card_response" {
                    let document: String = sqlx::query_scalar(
                        "SELECT document FROM expert_cards WHERE account_id=? AND id=?",
                    )
                    .bind(account)
                    .bind(text(run, "trigger_record_id")?)
                    .fetch_one(&mut *transaction)
                    .await?;
                    let card: Value = serde_json::from_str(&document)?;
                    if card["response_action"] == "skip"
                        && card["step_id"] == step_id
                        && task["steps"][index]["status"] == "skipped"
                    {
                        transaction.commit().await?;
                        return Ok(task);
                    }
                }
                if input["authorization_message_id"] != run["trigger_message_id"] {
                    return Err(Error::Invalid("跳过步骤需要本轮用户消息授权"));
                }
                task["steps"][index]["status"] = json!("skipped");
                task["steps"][index]["summary"] = json!(optional_text(input, "check_summary", "")?);
            }
            "update_step" => {
                let index = index.ok_or(Error::Invalid("专家步骤不存在"))?;
                let attempt_id = text(input, "attempt_id")?;
                if task["steps"][index]["latest_attempt_id"] != attempt_id {
                    return Err(Error::Invalid("步骤处理记录已更新，请重新读取"));
                }
                let status = text(input, "status")?;
                if ![
                    "running",
                    "waiting_user",
                    "waiting_tool",
                    "completed",
                    "failed",
                ]
                .contains(&status)
                {
                    return Err(Error::Invalid("步骤状态无效"));
                }
                let result = input.get("result").cloned().unwrap_or_else(|| json!({}));
                Self::validate_expert_result_in(
                    &mut transaction,
                    account,
                    text(run, "session_id")?,
                    &result,
                    status == "completed",
                )
                .await?;
                if status == "completed" {
                    Self::validate_expert_attempt_workflows_in(
                        &mut transaction,
                        account,
                        attempt_id,
                    )
                    .await?;
                }
                let mut attempt =
                    Self::expert_attempt_in(&mut transaction, account, attempt_id).await?;
                if attempt["expert_task_id"] != task["id"] || attempt["step_id"] != step_id {
                    return Err(Error::Invalid("步骤处理记录不匹配"));
                }
                attempt["status"] = json!(status);
                attempt["result"] = result;
                attempt["check_summary"] = json!(optional_text(input, "check_summary", "")?);
                if ["completed", "failed"].contains(&status) {
                    attempt["completed_at"] = json!(timestamp()?);
                }
                Self::save_expert_attempt_in(&mut transaction, account, &mut attempt).await?;
                task["steps"][index]["status"] = json!(status);
                task["steps"][index]["summary"] = attempt["check_summary"].clone();
                task["current_step_id"] = json!(step_id);
                task["status"] = json!(if ["waiting_user", "waiting_tool"].contains(&status) {
                    status
                } else {
                    "active"
                });
            }
            "update_task" => {
                let status = text(input, "status")?;
                if ![
                    "active",
                    "waiting_user",
                    "waiting_tool",
                    "paused",
                    "completed",
                    "cancelled",
                    "failed",
                ]
                .contains(&status)
                {
                    return Err(Error::Invalid("专家任务状态无效"));
                }
                if status == "completed"
                    && task["steps"]
                        .as_array()
                        .ok_or(Error::State("专家步骤格式无效"))?
                        .iter()
                        .any(|step| step["status"] != "completed" && step["status"] != "skipped")
                {
                    return Err(Error::Invalid("专家任务仍有未完成步骤"));
                }
                if status == "active"
                    && ["completed", "cancelled", "failed", "paused"]
                        .contains(&text(&task, "status")?)
                    && input["authorization_message_id"] != run["trigger_message_id"]
                {
                    return Err(Error::Invalid("恢复专家任务需要本轮用户授权"));
                }
                task["status"] = json!(status);
                task["summary"] = json!(optional_text(input, "check_summary", "")?);
                task["completed_at"] = if ["completed", "cancelled", "failed"].contains(&status) {
                    json!(timestamp()?)
                } else {
                    Value::Null
                };
            }
            _ => return Err(Error::Invalid("专家任务操作无效")),
        }
        Self::save_expert_task_in(&mut transaction, account, run, &mut task).await?;
        transaction.commit().await?;
        Ok(task)
    }
}
