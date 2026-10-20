use crate::{
    Error, Result, Store, assets::digest, error::timestamp, local_api::text, tools::RunTool,
};
use rig::message::ToolCall;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

impl Store {
    pub(crate) async fn resolve_tool_approval_in_session(
        &self,
        account: &str,
        session: &str,
        id: &str,
        approved: bool,
        edited_arguments: Option<Value>,
    ) -> Result<Value> {
        self.get_session(account, session).await?;
        let owner: Option<String> =
            sqlx::query_scalar("SELECT session_id FROM approvals WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        if owner.as_deref() != Some(session) {
            return Err(Error::NotFound);
        }
        self.resolve_tool_approval(account, id, approved, edited_arguments)
            .await
    }

    pub(crate) async fn pause_for_tool_approval(
        &self,
        account: &str,
        run_id: &str,
        record: &Value,
        tool: &RunTool,
        call: &ToolCall,
    ) -> Result<()> {
        let run = self.get_run(account, run_id).await?;
        let mut call = call.clone();
        if let Some(arguments) = record["approved_arguments"].as_object() {
            call.function.arguments = arguments.clone();
        }
        let arguments = Value::Object(call.function.arguments.clone());
        let validator = jsonschema::validator_for(&tool.definition.parameters)
            .map_err(|_| Error::Invalid("工具参数 Schema 无效"))?;
        if call.function.invalid_arguments.is_some() || !validator.is_valid(&arguments) {
            return Err(Error::Invalid("工具参数不符合 Schema"));
        }
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        let binding = digest(&serde_json::to_vec(&(&call, tool))?);
        let approval = json!({
            "id": id, "reason": "tool_approval", "message": format!("执行 {}，参数：{}", tool.display_name(), arguments),
            "status": "pending", "tool_call_id": record["id"], "binding": binding,
            "metadata": { "toolCallId": record["id"], "toolName": tool.display_name(), "toolAlias": tool.definition.name, "arguments": arguments, "schema": tool.definition.parameters },
        });
        let mut transaction = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE tool_calls SET status='waiting_approval',definition=?,updated_at=? WHERE account_id=? AND run_id=? AND id=? AND status='prepared'")
            .bind(serde_json::to_string(tool)?).bind(&now).bind(account).bind(run_id).bind(text(record, "id")?).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("工具审批状态已更改"));
        }
        sqlx::query("INSERT INTO approvals(id,account_id,session_id,run_id,created_at,document) VALUES(?,?,?,?,?,?)")
            .bind(&id).bind(account).bind(text(&run, "session_id")?).bind(run_id).bind(&now).bind(approval.to_string()).execute(&mut *transaction).await?;
        let changed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','waiting_approval','$.updated_at',?) WHERE account_id=? AND id=? AND status='running'")
            .bind(&now).bind(account).bind(run_id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("运行无法等待工具审批"));
        }
        Self::append_event_in(
            &mut transaction,
            account,
            text(&run, "session_id")?,
            run_id,
            json!({ "type": "CUSTOM", "name": "pixoma.approval_required", "value": approval }),
        )
        .await?;
        Self::append_event_in(&mut transaction, account, text(&run, "session_id")?, run_id, json!({
            "type": "RUN_FINISHED", "threadId": run["session_id"], "runId": run_id,
            "outcome": { "type": "interrupt", "interrupts": [{ "id": id, "reason": "tool_approval", "message": approval["message"], "metadata": approval["metadata"] }] },
        })).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn resolve_tool_approval(
        &self,
        account: &str,
        id: &str,
        approved: bool,
        edited_arguments: Option<Value>,
    ) -> Result<Value> {
        Self::check_account(account)?;
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query("SELECT a.document,a.run_id,t.call,t.definition,t.approved_arguments,t.status AS tool_status FROM approvals a JOIN tool_calls t ON t.run_id=a.run_id AND t.id=json_extract(a.document,'$.tool_call_id') AND t.account_id=a.account_id WHERE a.account_id=? AND a.id=?")
            .bind(account).bind(id).fetch_optional(&mut *transaction).await?.ok_or(Error::NotFound)?;
        let mut approval: Value = serde_json::from_str(row.get("document"))?;
        let run_id: String = row.get("run_id");
        if approval["status"] != "pending" {
            if approval["approved"] != approved
                || approval["edited_arguments"]
                    != edited_arguments.as_ref().unwrap_or(&Value::Null).clone()
            {
                return Err(Error::Invalid("审批已经答复，不能修改决定"));
            }
            transaction.commit().await?;
            return self.get_run(account, &run_id).await;
        }
        if row.get::<String, _>("tool_status") != "waiting_approval" {
            return Err(Error::Invalid("工具调用已经结束"));
        }
        let mut call: ToolCall = serde_json::from_str(row.get("call"))?;
        let previous_arguments: Option<String> = row.get("approved_arguments");
        if let Some(arguments) = &previous_arguments {
            call.function.arguments = serde_json::from_str(arguments)?;
        }
        let tool: RunTool = serde_json::from_str(
            &row.get::<Option<String>, _>("definition")
                .ok_or(Error::State("工具审批缺少定义"))?,
        )?;
        if approval["binding"] != digest(&serde_json::to_vec(&(&call, &tool))?) {
            return Err(Error::State("工具调用与审批内容不一致"));
        }
        if let Some(arguments) = &edited_arguments {
            if !approved {
                return Err(Error::Invalid("拒绝审批不能修改工具参数"));
            }
            let validator = jsonschema::validator_for(&tool.definition.parameters)
                .map_err(|_| Error::Invalid("工具参数 Schema 无效"))?;
            if !arguments.is_object() || !validator.is_valid(arguments) {
                return Err(Error::Invalid("修改后的工具参数不符合 Schema"));
            }
        }
        let changed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','queued','$.updated_at',?,'$.subscription_after_sequence',(SELECT coalesce(max(sequence),0) FROM run_events WHERE run_id=?)) WHERE account_id=? AND id=? AND status='waiting_approval'")
            .bind(&now).bind(&run_id).bind(account).bind(&run_id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Invalid("运行已经结束或审批状态已更改"));
        }
        sqlx::query("UPDATE tool_calls SET status=?,approved_arguments=?,updated_at=? WHERE account_id=? AND run_id=? AND id=? AND status='waiting_approval'")
            .bind(if approved { "approved" } else { "denied" }).bind(edited_arguments.as_ref().map(serde_json::to_string).transpose()?.or(previous_arguments))
            .bind(&now).bind(account).bind(&run_id).bind(text(&approval, "tool_call_id")?).execute(&mut *transaction).await?;
        approval["status"] = json!(if approved { "approved" } else { "denied" });
        approval["approved"] = json!(approved);
        approval["edited_arguments"] = edited_arguments.unwrap_or(Value::Null);
        approval["updated_at"] = json!(now);
        sqlx::query("UPDATE approvals SET document=? WHERE account_id=? AND id=?")
            .bind(approval.to_string())
            .bind(account)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        self.get_run(account, &run_id).await
    }
}
