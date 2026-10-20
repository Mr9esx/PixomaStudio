use crate::{Error, Result, Store, error::timestamp, local_api::text};
use rig::{
    completion::ToolDefinition,
    message::{ToolCall, ToolName, ToolResultContent, UserContent},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct RunTool {
    pub definition: ToolDefinition,
    pub target: ToolTarget,
    pub requires_approval: bool,
    #[serde(default)]
    pub read_only: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentRuntime;
    use std::path::PathBuf;

    #[tokio::test]
    async fn mcp_image_conversion_errors_preserve_the_received_response()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("mcp-image-result-")
            .tempdir_in(root)?;
        let database = directory.path().join("studio.sqlite");
        let store = Store::open(&database).await?;
        let account = "mcp-image-result";
        let model = store.save_model(account, None, json!({
            "name": "图片结果检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:18082/v1", "model": "image-check",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": true, "image_output": false, "streaming": true }
        })).await?;
        let session = store.create_session(account, "image-result").await?;
        let queued = store
            .queue_run(
                account,
                &session.id,
                "image-result",
                text(&model, "id")?,
                "读取图片",
                "auto_approve",
            )
            .await?;
        let run_id = text(&queued["run"], "id")?;
        assert!(store.claim_run(account, run_id).await?);
        let run = store.get_run(account, run_id).await?;
        let tool = RunTool {
            definition: ToolDefinition::new(
                ToolName::new("read_image")?,
                "读取图片",
                json!({ "type": "object" }),
            ),
            target: ToolTarget::Mcp {
                connector_id: "images".to_owned(),
                url: "http://127.0.0.1:18082/mcp".to_owned(),
                name: "read_image".to_owned(),
                credential_ref: None,
            },
            requires_approval: false,
            read_only: true,
        };
        let call = ToolCall::from_wire(
            "image-result",
            rig::message::ToolFunction::new(tool.definition.name.clone(), json!({})),
        );
        store.save_tool_part(account, run_id, 1, 0, &call).await?;
        let record = store.tool_call_records(account, run_id, 1).await?.remove(0);
        store
            .begin_tool_execution(account, run_id, &record, &tool, &call)
            .await?;
        let mut image = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(32, 32).write_to(&mut image, image::ImageFormat::Png)?;
        let response = serde_json::to_value(rmcp::model::CallToolResult::success(vec![
            rmcp::model::ContentBlock::image(STANDARD.encode(image.get_ref()), "image/x-png"),
        ]))?;
        let error = store
            .finish_tool_execution(account, &run, &record, &tool, &call, Ok(response.clone()))
            .await
            .expect_err("不支持的图片 MIME 需要明确失败");
        assert!(matches!(error, Error::Invalid("MCP 图片格式不受支持")));
        store
            .finish_run(account, run_id, "failed", Some(&error))
            .await?;
        store.close().await;
        let store = Store::open(&database).await?;
        let row = sqlx::query(
            "SELECT status,result,raw_result FROM tool_calls WHERE account_id=? AND run_id=?",
        )
        .bind(account)
        .bind(run_id)
        .fetch_one(&store.pool)
        .await?;
        assert_eq!(row.get::<String, _>("status"), "failed");
        assert_eq!(
            serde_json::from_str::<Value>(row.get("raw_result"))?,
            response
        );
        let UserContent::ToolResult(result) = serde_json::from_str(row.get("result"))? else {
            return Err("工具结果格式无效".into());
        };
        assert!(result.is_error);
        assert!(
            store
                .run_events_after(account, run_id, 0)
                .await?
                .iter()
                .any(|event| event["type"] == "TOOL_CALL_RESULT"
                    && event["payload"]["isError"] == true)
        );
        store.close().await;
        Ok(())
    }

    struct TestCredential(keyring::Entry);

    impl Drop for TestCredential {
        fn drop(&mut self) {
            self.0.delete_credential().expect("删除测试模型凭据失败");
        }
    }

    #[tokio::test]
    #[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接真实模型服务"]
    async fn real_tool_result_remains_unknown_after_an_interrupted_result_save()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let configuration: Value =
            serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("tool-recovery-")
            .tempdir_in(root)?;
        let database = directory.path().join("studio.sqlite");
        let mut store = Store::open(&database).await?;
        let account = "tool-recovery";
        let model = store.save_model(account, None, configuration).await?;
        let model_id = text(&model, "id")?;
        let reference: String = sqlx::query_scalar(
            "SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(model_id)
        .fetch_one(&store.pool)
        .await?;
        let _credential = TestCredential(keyring::Entry::new("com.pixoma.studio", &reference)?);
        let session = store.create_session(account, "recovery").await?;
        let marker = uuid::Uuid::new_v4().to_string();
        let prompt = format!(
            "请调用 create_text_asset 创建名为执行恢复检查的 Markdown 文档，正文严格为 {marker}，只调用一次该工具。"
        );
        let queued = store
            .queue_run(
                account,
                &session.id,
                "recovery",
                model_id,
                &prompt,
                "request_approval",
            )
            .await?;
        let run_id = text(&queued["run"], "id")?;
        let outcome = AgentRuntime::new(store.clone())
            .execute(account, run_id)
            .await?;
        assert_eq!(
            outcome["status"], "waiting_approval",
            "{}",
            outcome["error_message"]
        );
        let turn = store.run_checkpoint(account, run_id).await?.turn();
        let records = store.tool_call_records(account, run_id, turn).await?;
        assert_eq!(records.len(), 1);
        let call: ToolCall = serde_json::from_value(records[0]["call"].clone())?;
        assert_eq!(call.function.name.as_str(), "create_text_asset");
        let tool = store
            .run_tools(account, run_id)
            .await?
            .into_iter()
            .find(|tool| tool.definition.name == call.function.name)
            .ok_or("工具定义缺失")?;
        let detail = store.session_detail(account, &session.id).await?;
        let approval = text(&detail["pending_approvals"][0], "id")?;
        store
            .resolve_tool_approval(account, approval, true, None)
            .await?;
        assert!(store.claim_run(account, run_id).await?);
        let record = &store.tool_call_records(account, run_id, turn).await?[0];
        let (run, executing_call) = store
            .begin_tool_execution(account, run_id, record, &tool, &call)
            .await?;
        let arguments = Value::Object(executing_call.function.arguments.clone());
        let content = text(&arguments, "content")?.as_bytes().to_vec();
        assert!(text(&arguments, "content")?.contains(&marker));
        store
            .call_builtin(
                account,
                text(&run, "session_id")?,
                text(record, "id")?,
                &tool.definition.name,
                &arguments,
            )
            .await?;
        assert!(
            store
                .execute_local_tool(account, run_id, record, &tool, &call)
                .await
                .is_err()
        );
        assert!(
            store
                .tool_display_name(account, run_id, &ToolName::new("unknown_tool")?)
                .await
                .is_err()
        );
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 1);
        let interrupted = store.get_run(account, run_id).await?;
        assert_eq!(interrupted["status"], "failed");
        assert_eq!(interrupted["error_code"], "tool_result_unknown");
        let recovered = store.tool_call_records(account, run_id, turn).await?;
        assert_eq!(recovered[0]["status"], "result_unknown");
        assert!(recovered[0]["result"].is_null());
        let outcome = AgentRuntime::new(store.clone())
            .execute(account, run_id)
            .await?;
        assert_eq!(outcome["status"], "failed");
        assert!(
            store
                .execute_local_tool(account, run_id, &recovered[0], &tool, &call)
                .await
                .is_err()
        );
        let assets = store.list_session_assets(account, &session.id).await?;
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].versions.len(), 1);
        assert_eq!(
            store
                .read_asset(account, &assets[0].id, &assets[0].versions[0].id)
                .await?,
            content
        );
        assert_eq!(store.recover_interrupted_runs().await?, 0);
        let continued = store
            .queue_run(
                account,
                &session.id,
                "read-recovery-state",
                model_id,
                "只说明上一项工具调用的结果是否已经确认，保持原有文档。",
                "auto_approve",
            )
            .await?;
        let history = store
            .previous_run_history(account, text(&continued["run"], "id")?)
            .await?;
        rig::transcript::validate_canonical(&history)?;
        assert!(history.iter().any(|message| matches!(message, rig::message::Message::User { content } if content.iter().any(|content| matches!(content, UserContent::ToolResult(result) if result.is_error && result.content.iter().any(|content| matches!(content, ToolResultContent::Text(text) if text.text.contains("工具结果未知"))))))));
        assert_eq!(
            store.list_session_assets(account, &session.id).await?.len(),
            1
        );
        store.close().await;
        Ok(())
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ToolTarget {
    Builtin,
    Expert,
    Mcp {
        connector_id: String,
        url: String,
        name: String,
        credential_ref: Option<String>,
    },
}

impl RunTool {
    pub fn display_name(&self) -> &str {
        match &self.target {
            ToolTarget::Builtin | ToolTarget::Expert => self.definition.name.as_str(),
            ToolTarget::Mcp { name, .. } => name,
        }
    }
}

fn builtin_tools() -> Result<Vec<RunTool>> {
    let string = json!({ "type": "string", "minLength": 1 });
    [
        ("load_skill", "读取当前运行绑定的技能版本。省略 path 时读取 SKILL.md，path 为 . 时分页列出可读文件；offset 使用 Unicode 字符位置或目录序号，truncated 为 true 时继续读取 next。", json!({
            "type": "object", "properties": { "skill_id": string, "path": { "type": "string" }, "offset": { "type": "integer", "minimum": 0 } },
            "required": ["skill_id"], "additionalProperties": false
        })),
        ("list_assets", "列出当前会话的资产、版本和引用。", json!({ "type": "object", "properties": {}, "additionalProperties": false })),
        ("read_asset", "读取指定资产版本的文本内容。", json!({
            "type": "object", "properties": { "asset_id": string, "asset_version_id": string },
            "required": ["asset_id", "asset_version_id"], "additionalProperties": false
        })),
        ("create_text_asset", "在当前会话中创建 Markdown 文档并加入资产库。", json!({
            "type": "object", "properties": { "name": string, "content": { "type": "string", "minLength": 1, "maxLength": 1048576 } },
            "required": ["name", "content"], "additionalProperties": false
        })),
        ("update_text_asset", "更新手动文档并保留原有版本。", json!({
            "type": "object", "properties": { "asset_id": string, "content": { "type": "string", "minLength": 1, "maxLength": 1048576 } },
            "required": ["asset_id", "content"], "additionalProperties": false
        })),
    ].into_iter().map(|(name, description, schema)| Ok(RunTool {
        definition: ToolDefinition::new(ToolName::new(name).map_err(|_| Error::State("工具名称无效"))?, description, schema),
        target: ToolTarget::Builtin, requires_approval: false, read_only: matches!(name, "list_assets" | "read_asset" | "load_skill"),
    })).collect()
}

impl Store {
    pub(crate) async fn tool_display_name(
        &self,
        account: &str,
        id: &str,
        name: &ToolName,
    ) -> Result<String> {
        let tools = self.run_tools(account, id).await?;
        let tool = tools
            .iter()
            .find(|tool| tool.definition.name == *name)
            .ok_or(Error::Invalid("调用的工具未注册"))?;
        Ok(tool.display_name().to_owned())
    }

    pub(crate) async fn run_tools(&self, account: &str, id: &str) -> Result<Vec<RunTool>> {
        let run = self.get_run(account, id).await?;
        let saved: Option<String> =
            sqlx::query_scalar("SELECT tool_snapshot FROM runs WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_one(&self.pool)
                .await?;
        if let Some(saved) = saved {
            return Ok(serde_json::from_str(&saved)?);
        }
        let mut tools = builtin_tools()?;
        if run["expert_id"].as_str().is_some_and(|id| !id.is_empty()) {
            tools.extend(crate::expert_tasks::expert_tools()?);
        }
        tools.extend(self.mcp_run_tools(account).await?);
        for tool in &tools {
            jsonschema::validator_for(&tool.definition.parameters)
                .map_err(|_| Error::Invalid("工具参数 Schema 无效"))?;
        }
        sqlx::query(
            "UPDATE runs SET tool_snapshot=? WHERE account_id=? AND id=? AND tool_snapshot IS NULL",
        )
        .bind(serde_json::to_string(&tools)?)
        .bind(account)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(tools)
    }

    pub(crate) async fn save_tool_part(
        &self,
        account: &str,
        id: &str,
        turn: usize,
        part: usize,
        call: &ToolCall,
    ) -> Result<()> {
        self.get_run(account, id).await?;
        let now = timestamp()?;
        sqlx::query("INSERT INTO tool_calls(id,account_id,run_id,turn,part,call,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(format!("{id}-{turn}-{part}")).bind(account).bind(id).bind(turn as i64).bind(i64::try_from(part).map_err(|_| Error::State("模型输出内容块超过范围"))?)
            .bind(serde_json::to_string(call)?).bind(&now).bind(&now).execute(&self.pool).await?;
        Ok(())
    }

    pub(crate) async fn tool_call_records(
        &self,
        account: &str,
        id: &str,
        turn: usize,
    ) -> Result<Vec<Value>> {
        self.get_run(account, id).await?;
        let rows = sqlx::query("SELECT id,call,status,result,definition,approved_arguments FROM tool_calls WHERE account_id=? AND run_id=? AND turn=? ORDER BY part")
            .bind(account).bind(id).bind(turn as i64).fetch_all(&self.pool).await?;
        rows.into_iter().map(|row| Ok(json!({
            "id": row.get::<String, _>("id"), "call": serde_json::from_str::<Value>(row.get("call"))?,
            "status": row.get::<String, _>("status"),
            "result": row.get::<Option<String>, _>("result").map(|result| serde_json::from_str::<Value>(&result)).transpose()?,
            "definition": row.get::<Option<String>, _>("definition").map(|definition| serde_json::from_str::<Value>(&definition)).transpose()?,
            "approved_arguments": row.get::<Option<String>, _>("approved_arguments").map(|arguments| serde_json::from_str::<Value>(&arguments)).transpose()?,
        }))).collect()
    }

    async fn begin_tool_execution(
        &self,
        account: &str,
        run_id: &str,
        record: &Value,
        tool: &RunTool,
        original_call: &ToolCall,
    ) -> Result<(Value, ToolCall)> {
        let mut call = original_call.clone();
        if let Some(arguments) = record["approved_arguments"].as_object() {
            call.function.arguments = arguments.clone();
        }
        let run = self.get_run(account, run_id).await?;
        self.guard_expert_tool_write(account, &run, tool).await?;
        let arguments = Value::Object(call.function.arguments.clone());
        if call.function.invalid_arguments.is_some() {
            return Err(Error::Invalid("工具参数需要有效的 JSON 对象"));
        }
        let validator = jsonschema::validator_for(&tool.definition.parameters)
            .map_err(|_| Error::Invalid("工具参数 Schema 无效"))?;
        if !validator.is_valid(&arguments) {
            return Err(Error::Invalid("工具参数不符合 Schema"));
        }
        let id = text(record, "id")?;
        let started = timestamp()?;
        let changed = sqlx::query("UPDATE tool_calls SET status='executing',definition=?,updated_at=?,started_at=? WHERE account_id=? AND run_id=? AND id=? AND status IN ('prepared','approved')")
            .bind(serde_json::to_string(tool)?).bind(&started).bind(&started).bind(account).bind(run_id).bind(id).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("工具调用已经执行或结果未知"));
        }
        Ok((run, call))
    }

    pub(crate) async fn execute_local_tool(
        &self,
        account: &str,
        run_id: &str,
        record: &Value,
        tool: &RunTool,
        original_call: &ToolCall,
    ) -> Result<Option<UserContent>> {
        let mut call = original_call.clone();
        if let Some(arguments) = record["approved_arguments"].as_object() {
            call.function.arguments = arguments.clone();
        }
        if record["status"] != "waiting_workflow"
            && record["status"] != "waiting_mcp"
            && self
                .pause_for_workflow_input(account, run_id, record, tool, &call)
                .await?
        {
            return Ok(None);
        }
        let (run, call) =
            if record["status"] == "waiting_workflow" || record["status"] == "waiting_mcp" {
                (self.get_run(account, run_id).await?, call)
            } else {
                self.begin_tool_execution(account, run_id, record, tool, original_call)
                    .await?
            };
        let arguments = Value::Object(call.function.arguments.clone());
        let id = text(record, "id")?;
        let output = match &tool.target {
            ToolTarget::Builtin => {
                self.call_builtin(
                    account,
                    text(&run, "session_id")?,
                    id,
                    &tool.definition.name,
                    &arguments,
                )
                .await
            }
            ToolTarget::Expert => {
                self.call_expert_control(account, run_id, id, &tool.definition.name, &arguments)
                    .await
            }
            ToolTarget::Mcp { name, .. } if name == "run_workflow" => {
                let workflow = self
                    .workflow_for_call(account, run_id, tool, &call)
                    .await?
                    .ok_or(Error::State("工作流详情缺失"))?;
                self.execute_workflow(account, &run, id, tool, &call, &workflow)
                    .await
            }
            ToolTarget::Mcp { .. } => match self
                .execute_mcp_tool(account, run_id, id, tool, &call)
                .await
            {
                Ok(Some(output)) => Ok(output),
                Ok(None) => return Ok(None),
                Err(error) => Err(error),
            },
        };
        if let Ok(output) = &output
            && matches!(tool.target, ToolTarget::Builtin)
            && matches!(
                tool.definition.name.as_str(),
                "create_text_asset" | "update_text_asset"
            )
        {
            self.record_expert_asset(account, &run, output).await?;
        }
        self.finish_tool_execution(account, &run, record, tool, &call, output)
            .await
            .map(Some)
    }

    async fn finish_tool_execution(
        &self,
        account: &str,
        run: &Value,
        record: &Value,
        tool: &RunTool,
        call: &ToolCall,
        output: Result<Value>,
    ) -> Result<UserContent> {
        let run_id = text(run, "id")?;
        let id = text(record, "id")?;
        let result = match &output {
            Ok(output) if matches!(&tool.target, ToolTarget::Mcp { name, .. } if name != "run_workflow") => {
                crate::mcp::model_result_content(output).map(|content| call.result(content))
            }
            Ok(output) => Ok(call.result(vec![ToolResultContent::json(output.clone())])),
            Err(error) => Ok(call.error_result(vec![ToolResultContent::text(error.to_string())])),
        };
        let (result, conversion_error) = match result {
            Ok(result) => (result, None),
            Err(error) => (
                call.error_result(vec![ToolResultContent::text(error.to_string())]),
                Some(error),
            ),
        };
        let content = UserContent::ToolResult(result);
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query("UPDATE runs SET document=json_set(document,'$.status','running') WHERE account_id=? AND id=? AND status='waiting_workflow'")
            .bind(account).bind(run_id).execute(&mut *transaction).await?;
        sqlx::query("UPDATE tool_calls SET status=?,result=?,raw_result=?,updated_at=?,ended_at=? WHERE account_id=? AND run_id=? AND id=? AND status IN ('executing','waiting_workflow','waiting_mcp')")
            .bind(if conversion_error.is_some() { "failed" } else { match &output { Ok(_) => "succeeded", Err(Error::ToolResultUnknown(_)) => "result_unknown", Err(_) => "failed" } }).bind(serde_json::to_string(&content)?)
            .bind(output.as_ref().ok().map(serde_json::to_string).transpose()?).bind(&now)
            .bind(&now)
            .bind(account).bind(run_id).bind(id).execute(&mut *transaction).await?;
        let result_text = if let Some(error) = &conversion_error {
            json!({ "response": output.as_ref().map_err(|_| Error::State("工具响应缺失"))?, "model_content_error": error.to_string() }).to_string()
        } else {
            match &output {
                Ok(output) => serde_json::to_string(output)?,
                Err(error) => error.to_string(),
            }
        };
        Self::append_event_in(&mut transaction, account, text(run, "session_id")?, run_id, json!({
            "type": "TOOL_CALL_RESULT", "toolCallId": id, "messageId": format!("{id}-result"),
            "content": result_text, "role": "tool", "isError": output.is_err() || conversion_error.is_some(),
        })).await?;
        let message = json!({ "id": format!("{id}-result"), "session_id": run["session_id"], "run_id": run_id, "role": "tool", "tool_call_id": id, "content": [{ "type": "text", "text": result_text }], "created_at": now });
        sqlx::query(
            "INSERT INTO messages(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)",
        )
        .bind(format!("{id}-result"))
        .bind(account)
        .bind(text(run, "session_id")?)
        .bind(now)
        .bind(message.to_string())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        output?;
        if let Some(error) = conversion_error {
            return Err(error);
        }
        Ok(content)
    }

    pub(crate) async fn refuse_tool_while_waiting_for_expert_card(
        &self,
        account: &str,
        run_id: &str,
        record: &Value,
        call: &ToolCall,
    ) -> Result<UserContent> {
        let run = self.get_run(account, run_id).await?;
        let id = text(record, "id")?;
        let reason = "等待专家卡片答复，当前调用未执行";
        let content =
            UserContent::ToolResult(call.error_result(vec![ToolResultContent::text(reason)]));
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE tool_calls SET status='not_executed',result=?,updated_at=?,ended_at=? WHERE account_id=? AND run_id=? AND id=? AND status IN ('prepared','approved')")
            .bind(serde_json::to_string(&content)?).bind(&now).bind(&now).bind(account).bind(run_id).bind(id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("工具调用状态已更新"));
        }
        Self::append_event_in(&mut transaction, account, text(&run, "session_id")?, run_id, json!({
            "type": "TOOL_CALL_RESULT", "toolCallId": id, "messageId": format!("{id}-result"), "content": reason, "role": "tool", "isError": true,
        })).await?;
        let message = json!({ "id": format!("{id}-result"), "session_id": run["session_id"], "run_id": run_id, "role": "tool", "tool_call_id": id, "content": [{ "type": "text", "text": reason }], "created_at": now });
        sqlx::query(
            "INSERT INTO messages(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)",
        )
        .bind(format!("{id}-result"))
        .bind(account)
        .bind(text(&run, "session_id")?)
        .bind(now)
        .bind(message.to_string())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(content)
    }

    async fn call_builtin(
        &self,
        account: &str,
        session: &str,
        request: &str,
        name: &ToolName,
        input: &Value,
    ) -> Result<Value> {
        match name.as_str() {
            "load_skill" => self.load_run_skill(account, request, input).await,
            "list_assets" => Ok(serde_json::to_value(
                self.list_session_assets(account, session).await?,
            )?),
            "create_text_asset" => Ok(serde_json::to_value(
                self.create_text_asset_with_origin(
                    account,
                    session,
                    text(input, "name")?,
                    text(input, "content")?,
                    request,
                    "agent",
                )
                .await?,
            )?),
            "update_text_asset" => Ok(serde_json::to_value(
                self.update_text_asset_in_session(
                    account,
                    session,
                    text(input, "asset_id")?,
                    text(input, "content")?,
                    request,
                )
                .await?,
            )?),
            "read_asset" => {
                let asset = self.get_asset(account, text(input, "asset_id")?).await?;
                let version_id = text(input, "asset_version_id")?;
                let version = asset
                    .versions
                    .iter()
                    .find(|version| version.id == version_id)
                    .ok_or(Error::NotFound)?;
                if version.size_bytes > 1024 * 1024 {
                    return Err(Error::Invalid("文本资产超过 1 MiB"));
                }
                if !version.mime_type.starts_with("text/")
                    && version.mime_type != "application/json"
                {
                    return Err(Error::Invalid("当前工具仅读取文本资产"));
                }
                let content =
                    String::from_utf8(self.read_asset(account, &asset.id, &version.id).await?)
                        .map_err(|_| Error::Invalid("资产需要 UTF-8 文本编码"))?;
                Ok(
                    json!({ "asset_id": asset.id, "asset_version_id": version.id, "name": asset.name, "content": content }),
                )
            }
            _ => Err(Error::Invalid("调用的工具未注册")),
        }
    }
}
