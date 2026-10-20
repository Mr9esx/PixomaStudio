use crate::{
    Error, Result, Store,
    assets::digest,
    error::timestamp,
    local_api::text,
    mcp_inputs::McpInput,
    tools::{RunTool, ToolTarget},
};
use rig::message::ToolCall;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, CreateTaskResult, GetTaskResult, Task,
    TaskPayload,
};
use serde_json::{Value, json};

impl Store {
    pub(crate) async fn execute_mcp_tool(
        &self,
        account: &str,
        run: &str,
        id: &str,
        tool: &RunTool,
        call: &ToolCall,
    ) -> Result<Option<Value>> {
        let ToolTarget::Mcp { name, .. } = &tool.target else {
            return Err(Error::State("MCP 工具配置无效"));
        };
        let context = crate::mcp::McpCallContext::new(self.clone(), account, run, id, tool, call);
        let mut saved = self.mcp_execution(account, run, id, tool, call).await?;
        if saved.is_none() {
            match context
                .call_tool(
                    CallToolRequestParams::new(name.clone())
                        .with_arguments(call.function.arguments.clone()),
                )
                .await?
            {
                CallToolResponse::Complete(result) => {
                    if self
                        .mcp_execution(account, run, id, tool, call)
                        .await?
                        .is_some()
                    {
                        self.save_mcp_completion(account, run, id, tool, call, &result)
                            .await?;
                    }
                    return complete_result(result).map(Some);
                }
                CallToolResponse::Task(created) => {
                    self.save_mcp_task(account, run, id, tool, call, &created)
                        .await
                        .map_err(|error| {
                            Error::ToolResultUnknown(format!(
                                "MCP 任务 {} 无法保存：{error}",
                                created.task.task_id
                            ))
                        })?;
                    saved = self.mcp_execution(account, run, id, tool, call).await?;
                }
                CallToolResponse::InputRequired(input) => {
                    self.save_mcp_inputs(account, run, id, tool, call, &McpInput::Mrtr(input))
                        .await
                        .map_err(|error| {
                            Error::ToolResultUnknown(format!("MCP 继续状态无法保存：{error}"))
                        })?;
                    saved = self.mcp_execution(account, run, id, tool, call).await?;
                }
                _ => {
                    return Err(Error::ToolResultUnknown(
                        "MCP 服务返回了无法识别的结果".to_owned(),
                    ));
                }
            }
        }
        let mut saved = saved.ok_or(Error::State("MCP 任务记录缺失"))?;
        loop {
            match text(&saved, "status")? {
                "completed" => {
                    return complete_result(serde_json::from_value(saved["response"].clone())?)
                        .map(Some);
                }
                "failed" => return Err(Error::Mcp(saved["response"].to_string())),
                "cancelled" => return Err(Error::Mcp("MCP 远程任务已取消".to_owned())),
                "polling" => {}
                "input_required" => {
                    if self
                        .pause_for_mcp_input(account, run, id, tool, call)
                        .await?
                    {
                        return Ok(None);
                    }
                    saved = self
                        .mcp_execution(account, run, id, tool, call)
                        .await?
                        .ok_or(Error::State("MCP 继续记录缺失"))?;
                    continue;
                }
                "ready" if saved["input"]["source"] == "mrtr" => {
                    let request = self
                        .begin_mcp_continuation(account, run, id, tool, call)
                        .await?;
                    if saved["input"]["requests"]
                        .as_object()
                        .is_none_or(|requests| requests.is_empty())
                    {
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    }
                    let response = context.call_tool(request).await?;
                    match response {
                        CallToolResponse::Complete(result) => {
                            self.save_mcp_completion(account, run, id, tool, call, &result)
                                .await?
                        }
                        CallToolResponse::Task(created) => self
                            .save_mcp_task(account, run, id, tool, call, &created)
                            .await
                            .map_err(|error| {
                                Error::ToolResultUnknown(format!(
                                    "MCP 任务 {} 无法保存：{error}",
                                    created.task.task_id
                                ))
                            })?,
                        CallToolResponse::InputRequired(input) => self
                            .save_mcp_inputs(account, run, id, tool, call, &McpInput::Mrtr(input))
                            .await
                            .map_err(|error| {
                                Error::ToolResultUnknown(format!("MCP 继续状态无法保存：{error}"))
                            })?,
                        _ => {
                            return Err(Error::ToolResultUnknown(
                                "MCP 服务返回了无法识别的结果".to_owned(),
                            ));
                        }
                    }
                    saved = self
                        .mcp_execution(account, run, id, tool, call)
                        .await?
                        .ok_or(Error::State("MCP 继续记录缺失"))?;
                    continue;
                }
                "ready" if saved["input"]["source"] == "task" => {
                    let task: Task = serde_json::from_value(saved["task"].clone())?;
                    let responses = serde_json::from_value(saved["input"]["responses"].clone())?;
                    context.update_task(&task.task_id, responses).await?;
                    saved["task_responses"] = saved["input"]["responses"].clone();
                    saved["status"] = json!("polling");
                    self.update_mcp_execution(account, run, id, &saved).await?;
                }
                _ => return Err(Error::State("MCP 任务状态无效")),
            }
            let task: Task = serde_json::from_value(saved["task"].clone())?;
            tokio::time::sleep(std::time::Duration::from_millis(
                task.poll_interval_ms.unwrap_or(1000).max(1),
            ))
            .await;
            let result = context.get_task(&task.task_id).await?;
            if result.task.task.task_id != task.task_id {
                return Err(Error::Invalid("MCP 任务编号与查询结果不一致"));
            }
            match &result.task.payload {
                TaskPayload::Working
                | TaskPayload::Completed { .. }
                | TaskPayload::Failed { .. }
                | TaskPayload::Cancelled => {
                    self.save_mcp_task_result(account, run, id, tool, call, &result)
                        .await?;
                }
                TaskPayload::InputRequired { .. } => {
                    self.save_mcp_inputs(account, run, id, tool, call, &McpInput::Task(result))
                        .await?
                }
                _ => return Err(Error::Invalid("MCP 任务返回了无法识别的状态")),
            }
            saved = self
                .mcp_execution(account, run, id, tool, call)
                .await?
                .ok_or(Error::State("MCP 任务记录缺失"))?;
        }
    }

    pub(crate) async fn update_mcp_execution(
        &self,
        account: &str,
        run: &str,
        id: &str,
        document: &Value,
    ) -> Result<()> {
        let changed = sqlx::query("UPDATE mcp_executions SET document=?,updated_at=? WHERE account_id=? AND run_id=? AND id=? AND EXISTS(SELECT 1 FROM tool_calls t JOIN runs r ON r.account_id=t.account_id AND r.id=t.run_id WHERE t.account_id=mcp_executions.account_id AND t.run_id=mcp_executions.run_id AND t.id=mcp_executions.id AND t.status IN ('executing','waiting_mcp') AND r.status='running')")
            .bind(document.to_string()).bind(timestamp()?).bind(account).bind(run).bind(id).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("MCP 执行记录无法保存"));
        }
        Ok(())
    }

    pub(crate) async fn begin_mcp_continuation(
        &self,
        account: &str,
        run: &str,
        id: &str,
        tool: &RunTool,
        call: &ToolCall,
    ) -> Result<CallToolRequestParams> {
        let ToolTarget::Mcp { name, .. } = &tool.target else {
            return Err(Error::State("MCP 工具配置无效"));
        };
        let mut saved = self
            .mcp_execution(account, run, id, tool, call)
            .await?
            .ok_or(Error::State("MCP 继续记录缺失"))?;
        if saved["status"] != "ready" || saved["input"]["source"] != "mrtr" {
            return Err(Error::State("MCP 尚未收到完整答复"));
        }
        let input: rmcp::model::InputRequiredResult =
            serde_json::from_value(saved["input"]["response"].clone())?;
        let mut request = CallToolRequestParams::new(name.clone())
            .with_arguments(call.function.arguments.clone())
            .with_input_responses(serde_json::from_value(saved["input"]["responses"].clone())?);
        if let Some(state) = input.request_state {
            request = request.with_request_state(state);
        }
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query("UPDATE tool_calls SET status='executing',updated_at=? WHERE account_id=? AND run_id=? AND id=? AND status='waiting_mcp' AND EXISTS(SELECT 1 FROM runs r WHERE r.account_id=tool_calls.account_id AND r.id=tool_calls.run_id AND r.status='running')")
            .bind(timestamp()?).bind(account).bind(run).bind(id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("MCP 继续请求已经发送或运行已经结束"));
        }
        saved["status"] = json!("continuing");
        sqlx::query("UPDATE mcp_executions SET document=?,updated_at=? WHERE account_id=? AND run_id=? AND id=? AND json_extract(document,'$.status')='ready'")
            .bind(saved.to_string()).bind(timestamp()?).bind(account).bind(run).bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(request)
    }

    pub(crate) async fn save_mcp_completion(
        &self,
        account: &str,
        run: &str,
        id: &str,
        tool: &RunTool,
        call: &ToolCall,
        result: &CallToolResult,
    ) -> Result<()> {
        let mut saved = self
            .mcp_execution(account, run, id, tool, call)
            .await?
            .ok_or(Error::State("MCP 继续记录缺失"))?;
        saved["status"] = json!("completed");
        saved["response"] = serde_json::to_value(result)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query("UPDATE tool_calls SET status='waiting_mcp',updated_at=? WHERE account_id=? AND run_id=? AND id=? AND status='executing' AND EXISTS(SELECT 1 FROM runs r WHERE r.account_id=tool_calls.account_id AND r.id=tool_calls.run_id AND r.status='running')")
            .bind(timestamp()?).bind(account).bind(run).bind(id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("MCP 工具无法保存结果"));
        }
        sqlx::query("UPDATE mcp_executions SET document=?,updated_at=? WHERE account_id=? AND run_id=? AND id=?")
            .bind(saved.to_string()).bind(timestamp()?).bind(account).bind(run).bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn mcp_execution(
        &self,
        account: &str,
        run: &str,
        id: &str,
        tool: &RunTool,
        call: &ToolCall,
    ) -> Result<Option<Value>> {
        self.get_run(account, run).await?;
        let document: Option<String> = sqlx::query_scalar(
            "SELECT document FROM mcp_executions WHERE account_id=? AND run_id=? AND id=?",
        )
        .bind(account)
        .bind(run)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let document = document
            .map(|document| serde_json::from_str::<Value>(&document))
            .transpose()?;
        if let Some(document) = &document
            && document["binding"] != digest(&serde_json::to_vec(&(tool, call))?)
        {
            return Err(Error::Invalid("MCP 执行记录与工具调用不一致"));
        }
        Ok(document)
    }

    pub(crate) async fn save_mcp_task(
        &self,
        account: &str,
        run: &str,
        id: &str,
        tool: &RunTool,
        call: &ToolCall,
        created: &CreateTaskResult,
    ) -> Result<()> {
        self.get_run(account, run).await?;
        let task = &created.task;
        if task.task_id.trim().is_empty() {
            return Err(Error::Invalid("MCP 任务编号为空"));
        }
        let now = timestamp()?;
        let binding = digest(&serde_json::to_vec(&(tool, call))?);
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let previous: Option<String> = sqlx::query_scalar(
            "SELECT document FROM mcp_executions WHERE account_id=? AND run_id=? AND id=?",
        )
        .bind(account)
        .bind(run)
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?;
        let mut document = if let Some(previous) = previous {
            let previous: Value = serde_json::from_str(&previous)?;
            if previous["binding"] != binding
                || previous["task"]["taskId"]
                    .as_str()
                    .is_some_and(|id| id != task.task_id)
            {
                return Err(Error::Invalid("MCP 任务与保存记录不一致"));
            }
            previous
        } else {
            json!({ "binding": binding })
        };
        let changed = sqlx::query("UPDATE tool_calls SET status='waiting_mcp',updated_at=? WHERE account_id=? AND run_id=? AND id=? AND status IN ('executing','waiting_mcp') AND EXISTS(SELECT 1 FROM runs WHERE runs.account_id=tool_calls.account_id AND runs.id=tool_calls.run_id AND runs.status='running')")
            .bind(&now).bind(account).bind(run).bind(id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("工具调用无法等待 MCP 任务"));
        }
        document["status"] = json!("polling");
        document["task"] = serde_json::to_value(task)?;
        document["created_response"] = serde_json::to_value(created)?;
        sqlx::query("INSERT INTO mcp_executions(id,account_id,run_id,document,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(run_id,id) DO UPDATE SET document=excluded.document,updated_at=excluded.updated_at")
            .bind(id).bind(account).bind(run).bind(document.to_string()).bind(&now).bind(&now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn save_mcp_task_result(
        &self,
        account: &str,
        run: &str,
        id: &str,
        tool: &RunTool,
        call: &ToolCall,
        result: &GetTaskResult,
    ) -> Result<()> {
        let mut saved = self
            .mcp_execution(account, run, id, tool, call)
            .await?
            .ok_or(Error::State("MCP 任务记录缺失"))?;
        if saved["task"]["taskId"] != result.task.task.task_id {
            return Err(Error::Invalid("MCP 任务编号与查询结果不一致"));
        }
        let (status, response) = match &result.task.payload {
            TaskPayload::Working => ("polling", Value::Null),
            TaskPayload::Completed { result } => ("completed", Value::Object(result.clone())),
            TaskPayload::Failed { error } => ("failed", Value::Object(error.clone())),
            TaskPayload::Cancelled => ("cancelled", Value::Null),
            _ => return Err(Error::Invalid("MCP 任务尚未结束")),
        };
        saved["status"] = json!(status);
        saved["task"] = serde_json::to_value(&result.task.task)?;
        saved["response"] = response;
        saved["last_response"] = serde_json::to_value(result)?;
        let changed = sqlx::query("UPDATE mcp_executions SET document=?,updated_at=? WHERE account_id=? AND run_id=? AND id=? AND EXISTS(SELECT 1 FROM tool_calls t JOIN runs r ON r.account_id=t.account_id AND r.id=t.run_id WHERE t.account_id=mcp_executions.account_id AND t.run_id=mcp_executions.run_id AND t.id=mcp_executions.id AND t.status='waiting_mcp' AND r.status='running')")
            .bind(saved.to_string()).bind(timestamp()?).bind(account).bind(run).bind(id).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("MCP 任务无法保存结束结果"));
        }
        Ok(())
    }
}

fn complete_result(result: CallToolResult) -> Result<Value> {
    if result.is_error == Some(true) {
        return Err(Error::Mcp(serde_json::to_string(&result)?));
    }
    Ok(serde_json::to_value(result)?)
}

#[cfg(test)]
mod tests {
    use crate::{
        Store,
        local_api::text,
        tools::{RunTool, ToolTarget},
    };
    use rig::{
        completion::ToolDefinition,
        message::{ToolCall, ToolFunction, ToolName},
    };
    use rmcp::model::{Task, TaskStatus};
    use serde_json::json;
    use sqlx::Row;

    #[tokio::test]
    #[ignore = "需要 PIXOMA_TEST_TASK_MCP_URL 并连接官方 SDK TaskDemo 服务"]
    async fn real_sdk_task_keeps_its_id_and_computed_sum_after_reopening()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("mcp-task-live-")
            .tempdir_in(root)?;
        let database = directory.path().join("studio.sqlite");
        let mut store = Store::open(&database).await?;
        let account = "mcp-task-live";
        let model = store.save_model(account, None, json!({
            "name": "MCP 任务调用检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:18082/v1", "model": "task-state",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
        })).await?;
        store
            .save_connector(
                account,
                None,
                json!({
                    "name": "官方 SDK 任务服务", "url": std::env::var("PIXOMA_TEST_TASK_MCP_URL")?,
                    "credential": "", "enabled": true, "policy": "auto",
                }),
            )
            .await?;
        let tool = store
            .mcp_run_tools(account)
            .await?
            .into_iter()
            .find(|tool| tool.display_name() == "slow_sum")
            .ok_or("计算工具缺失")?;
        let session = store.create_session(account, "task-live").await?;
        let queued = store
            .queue_run(
                account,
                &session.id,
                "task-live",
                text(&model, "id")?,
                "计算数值",
                "full_access",
            )
            .await?;
        let run = text(&queued["run"], "id")?.to_owned();
        assert!(store.claim_run(account, &run).await?);
        let call = ToolCall::from_wire(
            "live-task",
            ToolFunction::new(tool.definition.name.clone(), json!({ "a": 17, "b": 23 })),
        );
        store.save_tool_part(account, &run, 1, 0, &call).await?;
        let record = store.tool_call_records(account, &run, 1).await?.remove(0);
        let id = text(&record, "id")?.to_owned();
        let initial_store = store.clone();
        let initial_run = run.clone();
        let initial_record = record.clone();
        let initial_tool = tool.clone();
        let initial_call = call.clone();
        let initial = tokio::spawn(async move {
            initial_store
                .execute_local_tool(
                    account,
                    &initial_run,
                    &initial_record,
                    &initial_tool,
                    &initial_call,
                )
                .await
        });
        let task_id = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let task: Option<String> = sqlx::query_scalar("SELECT json_extract(document,'$.task.taskId') FROM mcp_executions WHERE account_id=? AND run_id=? AND id=?")
                    .bind(account).bind(&run).bind(&id).fetch_optional(&store.pool).await?;
                if let Some(task) = task { return Ok::<_, crate::Error>(task); }
                if initial.is_finished() { return Err(crate::Error::State("MCP 调用没有保存任务编号")); }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await??;
        initial.abort();
        assert!(initial.await.expect_err("本地等待应已中断").is_cancelled());
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 1);
        assert!(store.claim_run(account, &run).await?);
        let record = store.tool_call_records(account, &run, 1).await?.remove(0);
        let result = store
            .execute_local_tool(account, &run, &record, &tool, &call)
            .await?
            .ok_or("任务没有返回结果")?;
        let rig::message::UserContent::ToolResult(result) = result else {
            return Err("工具结果格式无效".into());
        };
        assert!(!result.is_error);
        let output = result.content[0].as_json().ok_or("结构化结果缺失")?;
        assert_eq!(output["content"][0]["text"], (17 + 23).to_string());
        let saved = store
            .mcp_execution(account, &run, &id, &tool, &call)
            .await?
            .ok_or("任务记录缺失")?;
        assert_eq!(saved["task"]["taskId"], task_id);
        assert_eq!(saved["status"], "completed");
        assert_eq!(
            store.tool_call_records(account, &run, 1).await?[0]["status"],
            "succeeded"
        );
        store.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn saved_task_handles_survive_reopening_and_resume_the_original_task()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("mcp-task-state-")
            .tempdir_in(root)?;
        let database = directory.path().join("studio.sqlite");
        let mut store = Store::open(&database).await?;
        let account = "mcp-task-state";
        let model = store.save_model(account, None, json!({
            "name": "MCP 任务保存检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:18082/v1", "model": "task-state",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
        })).await?;
        let session = store.create_session(account, "task-state").await?;
        let queued = store
            .queue_run(
                account,
                &session.id,
                "task-state",
                text(&model, "id")?,
                "等待任务",
                "auto_approve",
            )
            .await?;
        let run = text(&queued["run"], "id")?;
        assert!(store.claim_run(account, run).await?);
        let tool = RunTool {
            definition: ToolDefinition::new(
                ToolName::new("read_task")?,
                "读取任务",
                json!({ "type": "object" }),
            ),
            target: ToolTarget::Mcp {
                connector_id: "tasks".to_owned(),
                url: "http://127.0.0.1:18082/mcp".to_owned(),
                name: "read_task".to_owned(),
                credential_ref: None,
            },
            requires_approval: false,
            read_only: true,
        };
        let call = ToolCall::from_wire(
            "task-state",
            ToolFunction::new(tool.definition.name.clone(), json!({})),
        );
        store.save_tool_part(account, run, 1, 0, &call).await?;
        let record = store.tool_call_records(account, run, 1).await?.remove(0);
        let id = text(&record, "id")?;
        sqlx::query("UPDATE tool_calls SET status='executing',definition=? WHERE account_id=? AND run_id=? AND id=?")
            .bind(serde_json::to_string(&tool)?).bind(account).bind(run).bind(id).execute(&store.pool).await?;
        let now = crate::error::timestamp()?;
        let task = Task::new(
            uuid::Uuid::new_v4().to_string(),
            TaskStatus::Working,
            &now,
            &now,
        )
        .with_poll_interval_ms(1250);
        let created = rmcp::model::CreateTaskResult::new(task.clone())
            .with_meta(serde_json::from_value(json!({ "trace": "created" }))?);
        store
            .save_mcp_task(account, run, id, &tool, &call, &created)
            .await?;
        let requests = serde_json::from_value(json!({
            "confirm": { "method": "elicitation/create", "params": {
                "mode": "form", "message": "确认任务输入", "requestedSchema": { "type": "object", "properties": { "confirmed": { "type": "boolean" } }, "required": ["confirmed"] }
            } },
            "link": { "method": "elicitation/create", "params": {
                "mode": "url", "message": "填写远程任务资料", "url": "https://example.com/task-input", "elicitationId": "task-input",
            } }
        }))?;
        let required = rmcp::model::GetTaskResult::new(rmcp::model::DetailedTask::new(
            task.clone(),
            rmcp::model::TaskPayload::InputRequired {
                input_requests: requests,
            },
        ));
        store
            .save_mcp_inputs(
                account,
                run,
                id,
                &tool,
                &call,
                &crate::mcp_inputs::McpInput::Task(required.clone()),
            )
            .await?;
        assert!(
            store
                .pause_for_mcp_input(account, run, id, &tool, &call)
                .await?
        );
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 0);
        assert_eq!(
            store.get_run(account, run).await?["status"],
            "waiting_clarification"
        );
        for (selected, remains) in [("value-0", true), ("accept", false)] {
            let pending: String = sqlx::query_scalar("SELECT document FROM clarifications WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending'")
                .bind(account).bind(run).fetch_one(&store.pool).await?;
            let pending: serde_json::Value = serde_json::from_str(&pending)?;
            let answer = json!({ "interruptId": pending["id"], "status": "resolved", "payload": { "selected": selected } });
            store
                .resolve_run_interrupt(account, &session.id, &answer)
                .await?;
            assert!(store.claim_run(account, run).await?);
            assert_eq!(
                store
                    .pause_for_mcp_input(account, run, id, &tool, &call)
                    .await?,
                remains
            );
        }
        let saved = store
            .mcp_execution(account, run, id, &tool, &call)
            .await?
            .ok_or("任务输入记录缺失")?;
        assert_eq!(
            saved["input"]["responses"]["confirm"],
            json!({ "action": "accept", "content": { "confirmed": true } })
        );
        assert_eq!(
            saved["input"]["responses"]["link"],
            json!({ "action": "accept" })
        );
        assert_eq!(saved["task"]["taskId"], task.task_id);
        let mut changed = required;
        if let rmcp::model::TaskPayload::InputRequired { input_requests } =
            &mut changed.task.payload
            && let Some(rmcp::model::InputRequest::Elicitation(request)) =
                input_requests.get_mut("confirm")
            && let rmcp::model::ElicitRequestParams::FormElicitationParams { message, .. } =
                &mut request.params
        {
            *message = "新的输入要求".to_owned();
        }
        assert!(
            store
                .save_mcp_inputs(
                    account,
                    run,
                    id,
                    &tool,
                    &call,
                    &crate::mcp_inputs::McpInput::Task(changed)
                )
                .await
                .is_err()
        );
        let mut working = rmcp::model::GetTaskResult::new(rmcp::model::DetailedTask::new(
            task.clone(),
            rmcp::model::TaskPayload::Working,
        ));
        working.meta = Some(serde_json::from_value(json!({ "trace": "working-query" }))?);
        store
            .save_mcp_task_result(account, run, id, &tool, &call, &working)
            .await?;
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 1);
        assert_eq!(store.get_run(account, run).await?["status"], "queued");
        let saved = store
            .mcp_execution(account, run, id, &tool, &call)
            .await?
            .ok_or("MCP 任务记录缺失")?;
        assert_eq!(saved["task"]["taskId"], task.task_id);
        assert_eq!(saved["task"]["pollIntervalMs"], 1250);
        assert_eq!(saved["created_response"]["_meta"]["trace"], "created");
        assert_eq!(saved["last_response"]["_meta"]["trace"], "working-query");
        assert!(
            store
                .mcp_execution("other-account", run, id, &tool, &call)
                .await
                .is_err()
        );
        let mut changed = call.clone();
        changed
            .function
            .arguments
            .insert("changed".to_owned(), json!(true));
        assert!(
            store
                .mcp_execution(account, run, id, &tool, &changed)
                .await
                .is_err()
        );
        let row =
            sqlx::query("SELECT status FROM tool_calls WHERE account_id=? AND run_id=? AND id=?")
                .bind(account)
                .bind(run)
                .bind(id)
                .fetch_one(&store.pool)
                .await?;
        assert_eq!(row.get::<String, _>("status"), "waiting_mcp");
        assert!(store.claim_run(account, run).await?);
        let result = rmcp::model::CallToolResult::success(vec![rmcp::model::ContentBlock::text(
            (17 + 23).to_string(),
        )]);
        let mut result = rmcp::model::GetTaskResult::new(rmcp::model::DetailedTask::new(
            task.clone(),
            rmcp::model::TaskPayload::Completed {
                result: serde_json::to_value(result)?
                    .as_object()
                    .ok_or("工具结果格式无效")?
                    .clone(),
            },
        ));
        result.meta = Some(serde_json::from_value(
            json!({ "trace": { "request": "completed-query" } }),
        )?);
        store
            .save_mcp_task_result(account, run, id, &tool, &call, &result)
            .await?;
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 1);
        assert_eq!(store.get_run(account, run).await?["status"], "queued");
        let saved = store
            .mcp_execution(account, run, id, &tool, &call)
            .await?
            .ok_or("MCP 结束结果缺失")?;
        assert_eq!(saved["status"], "completed");
        assert_eq!(saved["response"]["content"][0]["text"], "40");
        assert_eq!(
            saved["last_response"]["_meta"]["trace"]["request"],
            "completed-query"
        );
        assert_eq!(saved["created_response"]["_meta"]["trace"], "created");
        assert!(store.claim_run(account, run).await?);
        store.finish_run(account, run, "cancelled", None).await?;
        let record = store.tool_call_records(account, run, 1).await?.remove(0);
        assert_eq!(record["status"], "failed");
        assert!(record["result"].to_string().contains(&task.task_id));
        assert_eq!(
            store
                .mcp_execution(account, run, id, &tool, &call)
                .await?
                .ok_or("取消后任务记录缺失")?["task"]["status"],
            "completed"
        );
        store.close().await;
        Ok(())
    }
}
