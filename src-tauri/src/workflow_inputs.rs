use crate::{Error, Result, Store, assets::digest, context::RunContext, error::timestamp, local_api::text, tools::{RunTool, ToolTarget}};
use rig::message::ToolCall;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

fn valid_inputs(workflow: &Value, inputs: &Value) -> Result<bool> {
    let values = inputs.as_object().ok_or(Error::Invalid("工作流输入需要对象"))?;
    let fields = workflow["input_fields"].as_array().ok_or(Error::State("工作流输入字段缺失"))?;
    let complete = fields.iter().all(|field| field["required"] != true || field["key"].as_str().and_then(|key| values.get(key))
        .is_some_and(|value| !value.is_null() && value.as_str().is_none_or(|value| !value.trim().is_empty())));
    let validator = jsonschema::validator_for(&workflow["input_schema"]).map_err(|_| Error::State("工作流输入 Schema 无效"))?;
    Ok(complete && validator.is_valid(inputs))
}

impl Store {
    pub(crate) async fn workflow_transport_inputs(&self, workflow: &Value, inputs: &Value) -> Result<Value> {
        let inputs = inputs.as_object().ok_or(Error::Invalid("工作流输入需要对象"))?;
        for field in workflow["input_fields"].as_array().ok_or(Error::State("工作流输入字段缺失"))? {
            let kind = text(field, "type")?;
            if !["image", "video", "audio", "file"].contains(&kind) { continue; }
            let Some(value) = inputs.get(text(field, "key")?).filter(|value| !value.is_null()) else { continue; };
            for reference in value.as_array().map(Vec::as_slice).unwrap_or_else(||std::slice::from_ref(value)) {
                let asset = self.get_asset(text(reference, "asset_id")?).await?;
                let version = asset.versions.iter().find(|version| Some(version.id.as_str()) == reference["asset_version_id"].as_str()).ok_or(Error::NotFound)?;
                if (kind != "file" && !version.mime_type.starts_with(&format!("{kind}/"))) || version.size_bytes > 256 << 20 {
                    return Err(Error::Invalid("工作流输入类型无效或文件超过 256 MiB"));
                }
            }
        }
        Ok(Value::Object(inputs.clone()))
    }

    pub(crate) async fn workflow_for_call(&self, run_id: &str, tool: &RunTool, call: &ToolCall) -> Result<Option<Value>> {
        if !matches!(tool.target, ToolTarget::Builtin) || tool.definition.name.as_str() != "studio_run_workflow" { return Ok(None); }
        let arguments = Value::Object(call.function.arguments.clone());
        let id = text(&arguments, "workflow_id")?;
        let version = arguments.get("version_id").filter(|value| !value.is_null()).map(|value| value.as_str().filter(|value| !value.is_empty()).ok_or(Error::Invalid("工作流版本标识无效"))).transpose()?;
        let binding_key = digest(&serde_json::to_vec(&(id, version))?);
        let run = self.get_run(run_id).await?;
        if let Some(binding) = run["workflow_bindings"].get(&binding_key) { return Ok(Some(binding.clone())); }
        let snapshot: String = sqlx::query_scalar("SELECT context_snapshot FROM runs WHERE id=?").bind(run_id).fetch_one(&self.pool).await?;
        let context: RunContext = serde_json::from_str(&snapshot)?;
        let selected = context.workflows.iter().find(|workflow| workflow["id"] == id);
        if context.expert.is_some() && selected.is_none() { return Err(Error::Invalid("专家会话只能使用已绑定的工作流")); }
        let workflow = if let Some(selected) = selected {
            if version.is_some_and(|version| selected["version_id"] != version) { return Err(Error::Invalid("所选工作流版本与会话绑定不一致")); }
            selected.clone()
        } else { self.workflow_version(id, version).await? };
        let changed = sqlx::query("UPDATE runs SET document=json_set(document,?,json(?)) WHERE id=? AND status='running'")
            .bind(format!("$.workflow_bindings.{binding_key}")).bind(workflow.to_string()).bind(run_id).execute(&self.pool).await?;
        if changed.rows_affected() != 1 { return Err(Error::Cancelled); }
        Ok(Some(workflow))
    }

    pub async fn resolve_run_interrupt(&self, session: &str, response: &Value) -> Result<Value> {
        self.get_session(session).await?;
        let id = text(response, "interruptId")?;
        let reason: Option<String> = sqlx::query_scalar("SELECT json_extract(document,'$.reason') FROM clarifications WHERE session_id=? AND id=?")
            .bind(session).bind(id).fetch_optional(&self.pool).await?;
        match reason.as_deref() {
            Some("workflow_input") => return self.resolve_workflow_input(session, id, response).await,
            Some("input_required") => return self.resolve_mcp_input(session, id, response).await,
            Some(_) => return Err(Error::Invalid("补充输入类型无效")),
            None => {}
        }
        let approved = match text(response, "status")? {
            "resolved" if response["payload"] == true || response["payload"]["approved"] == true => true,
            "cancelled" => false,
            _ => return Err(Error::Invalid("审批答复无效")),
        };
        self.resolve_tool_approval_in_session(session, id, approved, response["payload"].get("editedArgs").cloned()).await
    }

    async fn resolve_workflow_input(&self, session: &str, id: &str, response: &Value) -> Result<Value> {
        let submitted = match text(response, "status")? {
            "resolved" => Some(response["payload"].get("inputs").filter(|inputs| inputs.is_object()).ok_or(Error::Invalid("缺少工作流输入"))?),
            "cancelled" => None,
            _ => return Err(Error::Invalid("工作流输入答复无效")),
        };
        let saved: String = sqlx::query_scalar("SELECT document FROM clarifications WHERE session_id=? AND id=?")
            .bind(session).bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let saved: Value = serde_json::from_str(&saved)?;
        let transport = if let Some(inputs) = submitted.filter(|_| saved["status"] == "pending") {
            if serde_json::to_vec(inputs)?.len() > 8 << 20 { return Err(Error::Invalid("工作流输入超过 8 MiB")); }
            Some(self.workflow_transport_inputs(&saved["metadata"]["workflow"], inputs).await?)
        } else { None };
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT c.document,c.run_id,t.call,t.definition,t.approved_arguments,t.status AS tool_status FROM clarifications c JOIN tool_calls t ON t.run_id=c.run_id AND t.id=json_extract(c.document,'$.tool_call_id') WHERE c.session_id=? AND c.id=?")
            .bind(session).bind(id).fetch_optional(&mut *transaction).await?.ok_or(Error::NotFound)?;
        let mut pending: Value = serde_json::from_str(row.get("document"))?;
        let run_id: String = row.get("run_id");
        if pending["status"] != "pending" {
            if pending["response"] != *response { return Err(Error::Invalid("工作流输入已经收到其他答复")); }
            transaction.commit().await?;
            return self.get_run(&run_id).await;
        }
        if pending["metadata"]["workflow"] != saved["metadata"]["workflow"] { return Err(Error::State("工作流输入要求已更改")); }
        if !["prepared", "approved"].contains(&row.get::<String, _>("tool_status").as_str()) { return Err(Error::Invalid("工作流工具调用已经结束")); }
        let mut call: ToolCall = serde_json::from_str(row.get("call"))?;
        if let Some(arguments) = row.get::<Option<String>, _>("approved_arguments") { call.function.arguments = serde_json::from_str(&arguments)?; }
        let tool: RunTool = serde_json::from_str(&row.get::<Option<String>, _>("definition").ok_or(Error::State("工作流工具定义缺失"))?)?;
        if pending["binding"] != digest(&serde_json::to_vec(&(&call, &tool, &pending["metadata"]["workflow"]))?) { return Err(Error::State("工作流输入与工具调用不一致")); }
        if let Some(inputs) = submitted {
            if !valid_inputs(&pending["metadata"]["workflow"], transport.as_ref().ok_or(Error::State("工作流输入尚未校验"))?)? { return Err(Error::Invalid("工作流输入不符合字段要求")); }
            call.function.arguments.insert("inputs".to_owned(), inputs.clone());
            let validator = jsonschema::validator_for(&tool.definition.parameters).map_err(|_| Error::State("工具参数 Schema 无效"))?;
            if !validator.is_valid(&Value::Object(call.function.arguments.clone())) { return Err(Error::Invalid("工作流输入不符合工具要求")); }
        }
        let now = timestamp()?;
        let changed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','queued','$.updated_at',?,'$.subscription_after_sequence',(SELECT coalesce(max(sequence),0) FROM run_events WHERE run_id=?)) WHERE id=? AND status='waiting_clarification'")
            .bind(&now).bind(&run_id).bind(&run_id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 { return Err(Error::Invalid("运行已经结束或输入状态已更改")); }
        let status = if submitted.is_some() { "answered" } else { "skipped" };
        sqlx::query("UPDATE tool_calls SET status=CASE WHEN ?=1 THEN status ELSE 'denied' END,approved_arguments=?,updated_at=? WHERE run_id=? AND id=? AND status IN ('prepared','approved')")
            .bind(submitted.is_some()).bind(if submitted.is_some() { Some(serde_json::to_string(&call.function.arguments)?) } else { None })
            .bind(&now).bind(&run_id).bind(text(&pending, "tool_call_id")?).execute(&mut *transaction).await?;
        pending["status"] = json!(status);
        pending["response"] = response.clone();
        pending["updated_at"] = json!(now);
        sqlx::query("UPDATE clarifications SET document=? WHERE id=?").bind(pending.to_string()).bind(id).execute(&mut *transaction).await?;
        sqlx::query("UPDATE messages SET document=json_set(document,'$.clarification.status',?,'$.clarification.workflow.submitted_inputs',json(?)) WHERE id=? AND json_extract(document,'$.clarification.id')=?")
            .bind(status).bind(submitted.unwrap_or(&Value::Null).to_string()).bind(text(&pending["metadata"], "messageId")?).bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_run(&run_id).await
    }

    pub(crate) async fn pause_for_workflow_input(&self, run_id: &str, record: &Value, tool: &RunTool, call: &ToolCall) -> Result<bool> {
        if !matches!(tool.target, ToolTarget::Builtin) || tool.definition.name.as_str() != "studio_run_workflow" { return Ok(false); }
        let arguments = Value::Object(call.function.arguments.clone());
        let validator = jsonschema::validator_for(&tool.definition.parameters).map_err(|_| Error::Invalid("工具参数 Schema 无效"))?;
        if call.function.invalid_arguments.is_some() || !validator.is_valid(&arguments) { return Err(Error::Invalid("工具参数不符合 Schema")); }
        let mut workflow = self.workflow_for_call(run_id, tool, call).await?.ok_or(Error::State("工作流版本缺失"))?;
        let transport = self.workflow_transport_inputs(&workflow, &arguments["inputs"]).await?;
        if valid_inputs(&workflow, &transport)? { return Ok(false); }
        workflow["suggested_inputs"] = arguments["inputs"].clone();
        let run = self.get_run(run_id).await?;
        let id = Uuid::new_v4().to_string();
        let message_id = format!("{}-workflow-input-{id}", text(record, "id")?);
        let now = timestamp()?;
        let question = "填写工作流输入";
        let pending = json!({ "id": id, "reason": "workflow_input", "message": question, "status": "pending", "tool_call_id": record["id"],
            "binding": digest(&serde_json::to_vec(&(call, tool, &workflow))?), "metadata": { "messageId": message_id, "workflow": workflow } });
        let message = json!({ "id": message_id, "session_id": run["session_id"], "run_id": run_id, "role": "assistant", "content": [{ "type": "text", "text": question }], "created_at": now,
            "clarification": { "id": id, "question": question, "options": [], "status": "pending", "workflow": workflow } });
        let mut transaction = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','waiting_clarification','$.updated_at',?) WHERE id=? AND status='running'")
            .bind(&now).bind(run_id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 { return Err(Error::State("运行无法等待工作流输入")); }
        let changed = sqlx::query("UPDATE tool_calls SET definition=?,updated_at=? WHERE run_id=? AND id=? AND status IN ('prepared','approved')")
            .bind(serde_json::to_string(tool)?).bind(&now).bind(run_id).bind(text(record, "id")?).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 { return Err(Error::State("工作流工具调用状态已更改")); }
        sqlx::query("INSERT INTO clarifications(id,session_id,run_id,created_at,document) VALUES(?,?,?,?,?)")
            .bind(&id).bind(text(&run, "session_id")?).bind(run_id).bind(&now).bind(pending.to_string()).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO messages(id,session_id,created_at,document) VALUES(?,?,?,?)")
            .bind(&message_id).bind(text(&run, "session_id")?).bind(&now).bind(message.to_string()).execute(&mut *transaction).await?;
        Self::append_event_in(&mut transaction, text(&run, "session_id")?, run_id, json!({ "type": "CUSTOM", "name": "pixoma.clarification_required", "value": pending })).await?;
        Self::append_event_in(&mut transaction, text(&run, "session_id")?, run_id, json!({ "type": "RUN_FINISHED", "threadId": run["session_id"], "runId": run_id,
            "outcome": { "type": "interrupt", "interrupts": [{ "id": id, "reason": "workflow_input", "message": question, "metadata": pending["metadata"] }] } })).await?;
        transaction.commit().await?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "workflow_inputs_tests.rs"]
mod tests;
