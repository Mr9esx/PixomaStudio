use crate::{
    Error, Result, Store, credentials, error::timestamp, local_api::text, runs::checkpoint_version,
    settings::ModelInput,
};
use rig::{
    AgentRun,
    completion::CompletionResponse,
    message::{AssistantContent, AssistantMessage, Message, ToolResultContent, UserContent},
};
use serde_json::{Value, json};
use sqlx::Row;

impl Store {
    pub(crate) async fn queued_sessions(&self, account: &str) -> Result<Vec<String>> {
        Self::check_account(account)?;
        Ok(sqlx::query_scalar("SELECT session_id FROM runs WHERE account_id=? AND status='queued' GROUP BY session_id ORDER BY min(created_at),session_id")
            .bind(account).fetch_all(&self.pool).await?)
    }

    pub(crate) async fn next_queued_run(
        &self,
        account: &str,
        session: &str,
    ) -> Result<Option<String>> {
        Self::check_account(account)?;
        Ok(sqlx::query_scalar("SELECT id FROM runs WHERE account_id=? AND session_id=? AND status='queued' ORDER BY created_at,id LIMIT 1")
            .bind(account).bind(session).fetch_optional(&self.pool).await?)
    }

    pub(crate) async fn unfinished_runs(&self, account: &str) -> Result<Vec<String>> {
        Self::check_account(account)?;
        Ok(sqlx::query_scalar("SELECT id FROM runs WHERE account_id=? AND status NOT IN ('succeeded','failed','cancelled') ORDER BY created_at,id")
            .bind(account).fetch_all(&self.pool).await?)
    }

    pub(crate) async fn run_model(&self, account: &str, id: &str) -> Result<ModelInput> {
        Self::check_account(account)?;
        self.check_run_model_enabled(account, id).await?;
        let snapshot: String =
            sqlx::query_scalar("SELECT model_snapshot FROM runs WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Error::NotFound)?;
        let snapshot: Value = serde_json::from_str(&snapshot)?;
        let mut configuration: ModelInput =
            serde_json::from_value(snapshot["configuration"].clone())?;
        if let Some(reference) = snapshot["credential_ref"].as_str() {
            configuration.api_key =
                credentials::load(reference)?.ok_or(Error::State("运行使用的模型凭据不存在"))?;
        }
        configuration.validate()?;
        Ok(configuration)
    }

    pub(crate) async fn check_run_model_enabled(&self, account: &str, id: &str) -> Result<()> {
        let enabled: bool = sqlx::query_scalar("SELECT json_extract(model_configs.document,'$.enabled')=1 AND json_extract(model_configs.document,'$.agent_enabled')=1 FROM runs JOIN model_configs ON model_configs.account_id=runs.account_id AND model_configs.id=json_extract(runs.document,'$.model_config_id') WHERE runs.account_id=? AND runs.id=?")
            .bind(account).bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        if !enabled {
            return Err(Error::Invalid("模型尚未启用 Agent 调用"));
        }
        Ok(())
    }

    pub(crate) async fn save_run_checkpoint(
        &self,
        account: &str,
        id: &str,
        checkpoint: &AgentRun,
    ) -> Result<()> {
        let changed = sqlx::query(
            "UPDATE runs SET checkpoint=?,checkpoint_version=? WHERE account_id=? AND id=? AND status='running'",
        )
        .bind(serde_json::to_string(checkpoint)?)
        .bind(checkpoint_version())
        .bind(account)
        .bind(id)
        .execute(&self.pool)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Cancelled);
        }
        Ok(())
    }

    pub(crate) async fn finish_run(
        &self,
        account: &str,
        id: &str,
        status: &str,
        error: Option<&Error>,
    ) -> Result<Value> {
        if !["succeeded", "failed", "cancelled"].contains(&status) {
            return Err(Error::Invalid("运行结束状态无效"));
        }
        let run = self.get_run(account, id).await?;
        let message = error.map(ToString::to_string);
        let code = error.map(|error| {
            if matches!(error, Error::ToolResultUnknown(_)) {
                "tool_result_unknown"
            } else {
                "agent_error"
            }
        });
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE runs SET document=json_set(document,'$.status',?,'$.completed_at',?,'$.updated_at',?,'$.error_message',?,'$.error_code',?) WHERE account_id=? AND id=? AND status NOT IN ('succeeded','failed','cancelled')",
        )
        .bind(status)
        .bind(&now)
        .bind(&now)
        .bind(&message)
        .bind(code)
        .bind(account)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
        if changed.rows_affected() == 1 {
            sqlx::query("UPDATE workflow_executions SET document=json_set(document,'$.status','failed','$.submission_state','result_unknown','$.error_message','工作流提交没有取得任务编号，禁止自动重复提交','$.completed_at',?) WHERE account_id=? AND run_id=? AND coalesce(json_extract(document,'$.task_id'),'')=''")
                .bind(&now).bind(account).bind(id).execute(&mut *transaction).await?;
            let waiting = sqlx::query("SELECT t.id,t.call,w.document FROM tool_calls t JOIN workflow_executions w ON w.account_id=t.account_id AND w.run_id=t.run_id AND w.id=t.id WHERE t.account_id=? AND t.run_id=? AND t.status='waiting_workflow'")
                .bind(account).bind(id).fetch_all(&mut *transaction).await?;
            for row in waiting {
                let call_id: String = row.get("id");
                let call: rig::message::ToolCall = serde_json::from_str(row.get("call"))?;
                let workflow: Value = serde_json::from_str(row.get("document"))?;
                let reason = format!(
                    "本地运行已结束，远程任务编号为 {}。远程状态继续保存在工作流记录中。",
                    text(&workflow, "task_id")?
                );
                let content = UserContent::ToolResult(
                    call.error_result(vec![ToolResultContent::text(&reason)]),
                );
                sqlx::query("UPDATE tool_calls SET status='failed',result=?,updated_at=?,ended_at=? WHERE account_id=? AND run_id=? AND id=? AND status='waiting_workflow'")
                    .bind(serde_json::to_string(&content)?).bind(&now).bind(&now).bind(account).bind(id).bind(&call_id).execute(&mut *transaction).await?;
                Self::append_event_in(&mut transaction, account, text(&run, "session_id")?, id, json!({
                    "type": "TOOL_CALL_RESULT", "toolCallId": call_id, "messageId": format!("{call_id}-result"), "content": reason, "role": "tool", "isError": true,
                })).await?;
                let message = json!({ "id": format!("{call_id}-result"), "session_id": run["session_id"], "run_id": id, "role": "tool", "tool_call_id": call_id, "content": [{ "type": "text", "text": reason }], "created_at": now });
                sqlx::query("INSERT INTO messages(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)")
                    .bind(format!("{call_id}-result")).bind(account).bind(text(&run, "session_id")?).bind(&now).bind(message.to_string()).execute(&mut *transaction).await?;
            }
            let waiting_mcp = sqlx::query("SELECT t.id,t.call,m.document FROM tool_calls t JOIN mcp_executions m ON m.account_id=t.account_id AND m.run_id=t.run_id AND m.id=t.id WHERE t.account_id=? AND t.run_id=? AND t.status='waiting_mcp'")
                .bind(account).bind(id).fetch_all(&mut *transaction).await?;
            for row in waiting_mcp {
                let call_id: String = row.get("id");
                let call: rig::message::ToolCall = serde_json::from_str(row.get("call"))?;
                let execution: Value = serde_json::from_str(row.get("document"))?;
                let reason = if let Some(task_id) = execution["task"]["taskId"].as_str() {
                    format!(
                        "本地运行已结束，MCP 任务编号为 {task_id}。服务返回的状态与结果保存在执行记录中。"
                    )
                } else {
                    "本地运行已结束，MCP 服务返回的继续记录已保留。".to_owned()
                };
                let content = UserContent::ToolResult(
                    call.error_result(vec![ToolResultContent::text(&reason)]),
                );
                sqlx::query("UPDATE tool_calls SET status='failed',result=?,updated_at=?,ended_at=? WHERE account_id=? AND run_id=? AND id=? AND status='waiting_mcp'")
                    .bind(serde_json::to_string(&content)?).bind(&now).bind(&now).bind(account).bind(id).bind(&call_id).execute(&mut *transaction).await?;
                Self::append_event_in(&mut transaction, account, text(&run, "session_id")?, id, json!({
                    "type": "TOOL_CALL_RESULT", "toolCallId": call_id, "messageId": format!("{call_id}-result"), "content": reason, "role": "tool", "isError": true,
                })).await?;
                let message = json!({ "id": format!("{call_id}-result"), "session_id": run["session_id"], "run_id": id, "role": "tool", "tool_call_id": call_id, "content": [{ "type": "text", "text": reason }], "created_at": now });
                sqlx::query("INSERT INTO messages(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)")
                    .bind(format!("{call_id}-result")).bind(account).bind(text(&run, "session_id")?).bind(&now).bind(message.to_string()).execute(&mut *transaction).await?;
            }
            sqlx::query("UPDATE approvals SET document=json_set(document,'$.status','cancelled') WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending'")
                .bind(account).bind(id).execute(&mut *transaction).await?;
            sqlx::query("UPDATE clarifications SET document=json_set(document,'$.status','cancelled','$.updated_at',?) WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending'")
                .bind(&now).bind(account).bind(id).execute(&mut *transaction).await?;
            sqlx::query("UPDATE messages SET document=json_set(document,'$.clarification.status','skipped') WHERE account_id=? AND json_extract(document,'$.run_id')=? AND json_extract(document,'$.clarification.status')='pending'")
                .bind(account).bind(id).execute(&mut *transaction).await?;
            sqlx::query("UPDATE tool_calls SET status='result_unknown',updated_at=?,ended_at=? WHERE account_id=? AND run_id=? AND status='executing'")
                .bind(&now).bind(&now).bind(account).bind(id).execute(&mut *transaction).await?;
            sqlx::query("UPDATE tool_calls SET status=?,updated_at=?,ended_at=? WHERE account_id=? AND run_id=? AND status IN ('prepared','waiting_approval','approved')")
                .bind(if status == "cancelled" { "denied" } else { "failed" }).bind(&now).bind(&now).bind(account).bind(id).execute(&mut *transaction).await?;
            sqlx::query("UPDATE model_requests SET document=json_set(document,'$.status',?,'$.ended_at',?,'$.error_message',?) WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='running'")
                .bind(if status == "cancelled" { "cancelled" } else { "failed" }).bind(&now).bind(&message).bind(account).bind(id).execute(&mut *transaction).await?;
            let event = if status == "failed" {
                json!({ "type": "RUN_ERROR", "message": message, "code": code })
            } else {
                json!({ "type": "RUN_FINISHED", "threadId": run["session_id"], "runId": id, "outcome": { "type": if status == "cancelled" { "cancelled" } else { "success" } } })
            };
            Self::append_event_in(
                &mut transaction,
                account,
                text(&run, "session_id")?,
                id,
                event,
            )
            .await?;
        }
        transaction.commit().await?;
        self.get_run(account, id).await
    }

    pub(crate) async fn save_assistant_turn(
        &self,
        account: &str,
        id: &str,
        turn: usize,
        response: &CompletionResponse,
    ) -> Result<()> {
        let run = self.get_run(account, id).await?;
        let mut content = Vec::new();
        let mut tool_calls = Vec::new();
        let tool_records = self.tool_call_records(account, id, turn).await?;
        let mut tool_index = 0;
        for part in &response.choice {
            match part {
                AssistantContent::Text(text) => {
                    content.push(json!({ "type": "text", "text": text.text }));
                }
                AssistantContent::Reasoning(reasoning) => {
                    content.push(json!({ "type": "reasoning", "text": reasoning.text }));
                }
                AssistantContent::ToolCall(call) => {
                    let record = tool_records
                        .get(tool_index)
                        .ok_or(Error::State("工具调用记录缺失"))?;
                    if record["call"] != serde_json::to_value(call)? {
                        return Err(Error::State("工具调用记录不一致"));
                    }
                    tool_index += 1;
                    tool_calls.push(json!({ "id": record["id"], "type": "function", "function": { "name": self.tool_display_name(account, id, &call.function.name).await?, "arguments": serde_json::to_string(&call.function.arguments)? } }));
                }
                _ => return Err(Error::Invalid("当前运行无法保存此类模型输出")),
            }
        }
        let message_id = format!("{id}-{turn}");
        let now = timestamp()?;
        let document = json!({ "id": message_id, "session_id": run["session_id"], "run_id": id, "role": "assistant", "content": content, "tool_calls": tool_calls, "created_at": now });
        sqlx::query(
            "INSERT INTO messages(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)",
        )
        .bind(message_id)
        .bind(account)
        .bind(text(&run, "session_id")?)
        .bind(now)
        .bind(document.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn previous_run_history(
        &self,
        account: &str,
        id: &str,
    ) -> Result<Vec<rig::message::Message>> {
        let run = self.get_run(account, id).await?;
        let previous = sqlx::query(
            "SELECT id,checkpoint,checkpoint_version,document FROM runs WHERE account_id=? AND session_id=? AND (created_at<? OR (created_at=? AND id<?)) ORDER BY created_at,id",
        )
        .bind(account)
        .bind(text(&run, "session_id")?)
        .bind(text(&run, "created_at")?)
        .bind(text(&run, "created_at")?)
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let mut history = Vec::new();
        for previous in previous {
            if previous
                .get::<Option<String>, _>("checkpoint_version")
                .as_deref()
                != Some(checkpoint_version().as_str())
            {
                return Err(Error::State("历史运行检查点与当前 Rig 版本不兼容"));
            }
            let record: Value = serde_json::from_str(previous.get("document"))?;
            if !["succeeded", "failed", "cancelled"].contains(&text(&record, "status")?) {
                return Err(Error::State("前一项会话运行尚未结束"));
            }
            let checkpoint: AgentRun = serde_json::from_str(
                &previous
                    .get::<Option<String>, _>("checkpoint")
                    .ok_or(Error::State("历史运行检查点不存在"))?,
            )?;
            let mut messages = checkpoint.messages().to_vec();
            if let Some(choice) = checkpoint.accepted_turn_choice() {
                messages.push(Message::Assistant(AssistantMessage::new(choice)));
            } else if record["status"] != "succeeded" {
                let prefix = format!("{}-{}-", text(&record, "id")?, checkpoint.turn());
                let events = self
                    .run_events_after(account, text(&record, "id")?, 0)
                    .await?;
                let partial: String = events
                    .iter()
                    .filter(|event| {
                        event["type"] == "TEXT_MESSAGE_CONTENT"
                            && event["payload"]["messageId"]
                                .as_str()
                                .is_some_and(|message| message.starts_with(&prefix))
                    })
                    .filter_map(|event| event["payload"]["delta"].as_str())
                    .collect();
                let already_saved = matches!(messages.last(), Some(Message::Assistant(assistant)) if rig::transcript::assistant_text_from_choice(&assistant.content) == partial);
                if !partial.is_empty() && !already_saved {
                    messages.push(Message::assistant(partial));
                }
            }
            self.complete_history_tool_results(account, text(&record, "id")?, &mut messages)
                .await?;
            history.extend(messages);
            if record["status"] != "succeeded" {
                history.push(Message::system(format!("历史运行状态记录：{}。保留的助手内容可能不完整。工具结果未知时需要核对已有作品与任务，不能自动重复执行。", serde_json::to_string(&json!({ "run_id": record["id"], "status": record["status"], "error_code": record["error_code"], "error_message": record["error_message"] }))?)));
            }
        }
        rig::transcript::validate_canonical(&history)
            .map_err(|_| Error::State("历史消息与工具结果不完整"))?;
        Ok(history)
    }

    async fn complete_history_tool_results(
        &self,
        account: &str,
        run: &str,
        messages: &mut Vec<Message>,
    ) -> Result<()> {
        let mut index = 0;
        while index < messages.len() {
            let calls = match &messages[index] {
                Message::Assistant(assistant) => {
                    assistant.tool_calls().cloned().collect::<Vec<_>>()
                }
                _ => Vec::new(),
            };
            let mut missing = Vec::new();
            for call in calls {
                if messages.get(index + 1).is_some_and(|message| matches!(message, Message::User { content } if content.iter().any(|content| matches!(content, UserContent::ToolResult(result) if result.call == call.id)))) { continue; }
                let record = sqlx::query("SELECT status,result FROM tool_calls WHERE account_id=? AND run_id=? AND call=?")
                    .bind(account).bind(run).bind(serde_json::to_string(&call)?).fetch_optional(&self.pool).await?.ok_or(Error::State("历史工具调用记录缺失"))?;
                let result = match record.get::<Option<String>, _>("result") {
                    Some(result) => serde_json::from_str::<UserContent>(&result)?,
                    None => {
                        let status: String = record.get("status");
                        let message = match status.as_str() {
                            "result_unknown" => {
                                "工具结果未知，需要核对已有作品与任务，不能自动重复执行".to_owned()
                            }
                            "failed" | "denied" | "not_executed" => {
                                format!("工具调用没有完成，保存状态为 {status}")
                            }
                            _ => return Err(Error::State("历史工具调用尚未确认结束")),
                        };
                        UserContent::ToolResult(
                            call.error_result(vec![ToolResultContent::text(message)]),
                        )
                    }
                };
                if !matches!(&result, UserContent::ToolResult(result) if result.call == call.id) {
                    return Err(Error::State("历史工具结果与调用不匹配"));
                }
                missing.push(result);
            }
            if !missing.is_empty() {
                match messages.get_mut(index + 1) {
                    Some(Message::User { content }) => content.extend(missing),
                    _ => messages.insert(index + 1, Message::User { content: missing }),
                }
            }
            index += 1;
        }
        Ok(())
    }
}
