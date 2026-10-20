use crate::{
    Error, Result, Store,
    assets::digest,
    context::RunContext,
    error::timestamp,
    local_api::text,
    tools::{RunTool, ToolTarget},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rig::message::ToolCall;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

fn valid_inputs(workflow: &Value, inputs: &Value) -> Result<bool> {
    let values = inputs
        .as_object()
        .ok_or(Error::Invalid("工作流输入需要对象"))?;
    let fields = workflow["input_fields"]
        .as_array()
        .ok_or(Error::State("工作流输入字段缺失"))?;
    let complete = fields.iter().all(|field| {
        field["required"] != true
            || field["key"]
                .as_str()
                .and_then(|key| values.get(key))
                .is_some_and(|value| {
                    !value.is_null() && value.as_str().is_none_or(|value| !value.trim().is_empty())
                })
    });
    let validator = jsonschema::validator_for(&workflow["input_schema"])
        .map_err(|_| Error::State("工作流输入 Schema 无效"))?;
    Ok(complete && validator.is_valid(inputs))
}

impl Store {
    pub(crate) async fn workflow_transport_inputs(
        &self,
        account: &str,
        workflow: &Value,
        inputs: &Value,
    ) -> Result<Value> {
        let mut inputs = inputs
            .as_object()
            .cloned()
            .ok_or(Error::Invalid("工作流输入需要对象"))?;
        for field in workflow["input_fields"]
            .as_array()
            .ok_or(Error::State("工作流输入字段缺失"))?
        {
            if !matches!(field["type"].as_str(), Some("image" | "video")) {
                continue;
            }
            let key = text(field, "key")?;
            let Some(value) = inputs.get(key).filter(|value| !value.is_null()) else {
                continue;
            };
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Reference {
                asset_id: String,
                asset_version_id: String,
            }
            let reference: Reference = serde_json::from_value(value.clone())
                .map_err(|_| Error::Invalid("媒体输入需要本地资产版本"))?;
            let asset = self.get_asset(account, &reference.asset_id).await?;
            let version = asset
                .versions
                .iter()
                .find(|version| version.id == reference.asset_version_id)
                .ok_or(Error::NotFound)?;
            if asset.kind != field["type"] || version.size_bytes > 8 << 20 {
                return Err(Error::Invalid("工作流媒体类型无效或文件超过 8 MiB"));
            }
            let bytes = self.read_asset(account, &asset.id, &version.id).await?;
            inputs.insert(
                key.to_owned(),
                json!(format!(
                    "data:{};base64,{}",
                    version.mime_type,
                    STANDARD.encode(bytes)
                )),
            );
        }
        Ok(Value::Object(inputs))
    }

    pub(crate) async fn workflow_for_call(
        &self,
        account: &str,
        run_id: &str,
        tool: &RunTool,
        call: &ToolCall,
    ) -> Result<Option<Value>> {
        let ToolTarget::Mcp {
            name,
            url,
            connector_id,
            ..
        } = &tool.target
        else {
            return Ok(None);
        };
        if name != "run_workflow" {
            return Ok(None);
        }
        let Some(id) = call
            .function
            .arguments
            .get("case_id")
            .and_then(Value::as_u64)
        else {
            return Err(Error::Invalid("工作流标识无效"));
        };
        let binding_key = digest(&serde_json::to_vec(&(connector_id, url, id))?);
        let run = self.get_run(account, run_id).await?;
        if let Some(binding) = run["workflow_bindings"].get(&binding_key) {
            return Ok(Some(binding.clone()));
        }
        let snapshot: String =
            sqlx::query_scalar("SELECT context_snapshot FROM runs WHERE account_id=? AND id=?")
                .bind(account)
                .bind(run_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Error::NotFound)?;
        let context: RunContext = serde_json::from_str(&snapshot)?;
        let connector = url::Url::parse(url).map_err(|_| Error::State("MCP 地址无效"))?;
        let mut catalog = None;
        for workflow in context.workflows {
            let source = url::Url::parse(text(&workflow, "source_url")?)
                .map_err(|_| Error::State("工作流来源地址无效"))?;
            if workflow["id"].as_str() == Some(id.to_string().as_str())
                && source.origin() == connector.origin()
            {
                catalog = Some(workflow);
                break;
            }
        }
        let descriptor = self.run_tools(account, run_id).await?.into_iter().find(|candidate| {
            matches!(&candidate.target, ToolTarget::Mcp { connector_id: candidate_id, name, .. } if candidate_id == connector_id && name == "get_workflow")
        }).ok_or(Error::Invalid("MCP 服务没有提供工作流详情工具"))?;
        let response = self
            .call_mcp_tool(
                account,
                &descriptor,
                &serde_json::Map::from_iter([("case_id".to_owned(), json!(id))]),
            )
            .await?;
        let description = response
            .get("structuredContent")
            .filter(|value| value.is_object())
            .ok_or(Error::Invalid("工作流详情缺少结构化内容"))?;
        if description["id"] != id || description["enabled"] != true {
            return Err(Error::Invalid("工作流标识不匹配或尚未启用"));
        }
        let fields = description["inputs"]
            .as_array()
            .ok_or(Error::Invalid("工作流输入字段缺失"))?;
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for field in fields {
            let key = text(field, "key")?;
            let kind = match text(field, "type")? {
                "number" => "number",
                "boolean" => "boolean",
                _ => "string",
            };
            properties.insert(key.to_owned(), json!({ "type": kind }));
            if field["required"] == true {
                required.push(key);
            }
        }
        let mut workflow = json!({
            "id": id.to_string(), "name": text(description, "name")?,
            "description": description.get("description").and_then(Value::as_str).unwrap_or(""),
            "source_url": url, "source": "mcp", "input_fields": fields, "output_fields": description["outputs"],
            "input_schema": { "type": "object", "properties": properties, "required": required },
        });
        if let Some(catalog) = catalog {
            if catalog["input_fields"] == description["inputs"] {
                workflow["input_schema"] = catalog["input_schema"].clone();
            }
            workflow["preview"] = catalog["preview"].clone();
            workflow["catalog_version_id"] = catalog["version_id"].clone();
            workflow["catalog_content_digest"] = catalog["content_digest"].clone();
        }
        workflow["content_digest"] = json!(digest(&serde_json::to_vec(&workflow)?));
        sqlx::query(
            "UPDATE runs SET document=json_set(document,?,json(?)) WHERE account_id=? AND id=?",
        )
        .bind(format!("$.workflow_bindings.{binding_key}"))
        .bind(workflow.to_string())
        .bind(account)
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        Ok(Some(workflow))
    }

    pub async fn resolve_run_interrupt(
        &self,
        account: &str,
        session: &str,
        response: &Value,
    ) -> Result<Value> {
        self.get_session(account, session).await?;
        let id = text(response, "interruptId")?;
        let workflow: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clarifications WHERE account_id=? AND session_id=? AND id=? AND json_extract(document,'$.reason')='workflow_input')")
            .bind(account).bind(session).bind(id).fetch_one(&self.pool).await?;
        if workflow {
            return self
                .resolve_workflow_input(account, session, id, response)
                .await;
        }
        let mcp: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clarifications WHERE account_id=? AND session_id=? AND id=? AND json_extract(document,'$.reason')='input_required')")
            .bind(account).bind(session).bind(id).fetch_one(&self.pool).await?;
        if mcp {
            return self.resolve_mcp_input(account, session, id, response).await;
        }
        let approved = match text(response, "status")? {
            "resolved"
                if response["payload"] == true || response["payload"]["approved"] == true =>
            {
                true
            }
            "cancelled" => false,
            _ => return Err(Error::Invalid("审批答复无效")),
        };
        self.resolve_tool_approval_in_session(
            account,
            session,
            id,
            approved,
            response["payload"].get("editedArgs").cloned(),
        )
        .await
    }

    async fn resolve_workflow_input(
        &self,
        account: &str,
        session: &str,
        id: &str,
        response: &Value,
    ) -> Result<Value> {
        let submitted = match text(response, "status")? {
            "resolved" => Some(
                response["payload"]
                    .get("inputs")
                    .filter(|inputs| inputs.is_object())
                    .ok_or(Error::Invalid("缺少工作流输入"))?,
            ),
            "cancelled" => None,
            _ => return Err(Error::Invalid("工作流输入答复无效")),
        };
        let saved: String = sqlx::query_scalar(
            "SELECT document FROM clarifications WHERE account_id=? AND session_id=? AND id=?",
        )
        .bind(account)
        .bind(session)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)?;
        let saved: Value = serde_json::from_str(&saved)?;
        let transport = if let Some(inputs) = submitted.filter(|_| saved["status"] == "pending") {
            if serde_json::to_vec(inputs)?.len() > 8 << 20 {
                return Err(Error::Invalid("工作流输入超过 8 MiB"));
            }
            Some(
                self.workflow_transport_inputs(account, &saved["metadata"]["workflow"], inputs)
                    .await?,
            )
        } else {
            None
        };
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT c.document,c.run_id,t.call,t.definition,t.approved_arguments,t.status AS tool_status FROM clarifications c JOIN tool_calls t ON t.run_id=c.run_id AND t.account_id=c.account_id AND t.id=json_extract(c.document,'$.tool_call_id') WHERE c.account_id=? AND c.session_id=? AND c.id=?")
            .bind(account).bind(session).bind(id).fetch_optional(&mut *transaction).await?.ok_or(Error::NotFound)?;
        let mut pending: Value = serde_json::from_str(row.get("document"))?;
        let run_id: String = row.get("run_id");
        if pending["status"] != "pending" {
            if pending["response"] != *response {
                return Err(Error::Invalid("工作流输入已经收到其他答复"));
            }
            transaction.commit().await?;
            return self.get_run(account, &run_id).await;
        }
        if pending["metadata"]["workflow"] != saved["metadata"]["workflow"] {
            return Err(Error::State("工作流输入要求已更改"));
        }
        if !["prepared", "approved"].contains(&row.get::<String, _>("tool_status").as_str()) {
            return Err(Error::Invalid("工作流工具调用已经结束"));
        }
        let mut call: ToolCall = serde_json::from_str(row.get("call"))?;
        if let Some(arguments) = row.get::<Option<String>, _>("approved_arguments") {
            call.function.arguments = serde_json::from_str(&arguments)?;
        }
        let tool: RunTool = serde_json::from_str(
            &row.get::<Option<String>, _>("definition")
                .ok_or(Error::State("工作流工具定义缺失"))?,
        )?;
        if pending["binding"]
            != digest(&serde_json::to_vec(&(
                &call,
                &tool,
                &pending["metadata"]["workflow"],
            ))?)
        {
            return Err(Error::State("工作流输入与工具调用不一致"));
        }
        if let Some(inputs) = submitted {
            if !valid_inputs(
                &pending["metadata"]["workflow"],
                transport
                    .as_ref()
                    .ok_or(Error::State("工作流输入尚未校验"))?,
            )? {
                return Err(Error::Invalid("工作流输入不符合字段要求"));
            }
            call.function
                .arguments
                .insert("inputs".to_owned(), inputs.clone());
            let validator = jsonschema::validator_for(&tool.definition.parameters)
                .map_err(|_| Error::State("工具参数 Schema 无效"))?;
            if !validator.is_valid(&Value::Object(call.function.arguments.clone())) {
                return Err(Error::Invalid("工作流输入不符合工具要求"));
            }
        }
        let now = timestamp()?;
        let changed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','queued','$.updated_at',?,'$.subscription_after_sequence',(SELECT coalesce(max(sequence),0) FROM run_events WHERE run_id=?)) WHERE account_id=? AND id=? AND status='waiting_clarification'")
            .bind(&now).bind(&run_id).bind(account).bind(&run_id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Invalid("运行已经结束或输入状态已更改"));
        }
        let status = if submitted.is_some() {
            "answered"
        } else {
            "skipped"
        };
        sqlx::query("UPDATE tool_calls SET status=CASE WHEN ?=1 THEN status ELSE 'denied' END,approved_arguments=?,updated_at=? WHERE account_id=? AND run_id=? AND id=? AND status IN ('prepared','approved')")
            .bind(submitted.is_some()).bind(if submitted.is_some() { Some(serde_json::to_string(&call.function.arguments)?) } else { None })
            .bind(&now).bind(account).bind(&run_id).bind(text(&pending, "tool_call_id")?).execute(&mut *transaction).await?;
        pending["status"] = json!(status);
        pending["response"] = response.clone();
        pending["updated_at"] = json!(now);
        sqlx::query("UPDATE clarifications SET document=? WHERE account_id=? AND id=?")
            .bind(pending.to_string())
            .bind(account)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE messages SET document=json_set(document,'$.clarification.status',?,'$.clarification.workflow.submitted_inputs',json(?)) WHERE account_id=? AND id=? AND json_extract(document,'$.clarification.id')=?")
            .bind(status).bind(submitted.unwrap_or(&Value::Null).to_string()).bind(account).bind(text(&pending["metadata"], "messageId")?).bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_run(account, &run_id).await
    }

    pub(crate) async fn pause_for_workflow_input(
        &self,
        account: &str,
        run_id: &str,
        record: &Value,
        tool: &RunTool,
        call: &ToolCall,
    ) -> Result<bool> {
        let ToolTarget::Mcp { name, .. } = &tool.target else {
            return Ok(false);
        };
        if name != "run_workflow" {
            return Ok(false);
        }
        let arguments = Value::Object(call.function.arguments.clone());
        let validator = jsonschema::validator_for(&tool.definition.parameters)
            .map_err(|_| Error::Invalid("工具参数 Schema 无效"))?;
        if call.function.invalid_arguments.is_some() || !validator.is_valid(&arguments) {
            return Err(Error::Invalid("工具参数不符合 Schema"));
        }
        let mut workflow = self
            .workflow_for_call(account, run_id, tool, call)
            .await?
            .ok_or(Error::State("工作流详情缺失"))?;
        let transport = self
            .workflow_transport_inputs(account, &workflow, &arguments["inputs"])
            .await?;
        if valid_inputs(&workflow, &transport)? {
            return Ok(false);
        }
        workflow["suggested_inputs"] = arguments["inputs"].clone();
        let run = self.get_run(account, run_id).await?;
        let id = Uuid::new_v4().to_string();
        let message_id = format!("{}-workflow-input-{id}", text(record, "id")?);
        let now = timestamp()?;
        let question = "填写工作流输入";
        let pending = json!({
            "id": id, "reason": "workflow_input", "message": question, "status": "pending",
            "tool_call_id": record["id"],
            "binding": digest(&serde_json::to_vec(&(call, tool, &workflow))?),
            "metadata": { "messageId": message_id, "workflow": workflow },
        });
        let message = json!({
            "id": message_id, "session_id": run["session_id"], "run_id": run_id, "role": "assistant",
            "content": [{ "type": "text", "text": question }], "created_at": now,
            "clarification": { "id": id, "question": question, "options": [], "status": "pending", "workflow": workflow },
        });
        let mut transaction = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','waiting_clarification','$.updated_at',?) WHERE account_id=? AND id=? AND status='running'")
            .bind(&now).bind(account).bind(run_id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("运行无法等待工作流输入"));
        }
        let changed = sqlx::query("UPDATE tool_calls SET definition=?,updated_at=? WHERE account_id=? AND run_id=? AND id=? AND status IN ('prepared','approved')")
            .bind(serde_json::to_string(tool)?).bind(&now).bind(account).bind(run_id).bind(text(record, "id")?).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("工作流工具调用状态已更改"));
        }
        sqlx::query("INSERT INTO clarifications(id,account_id,session_id,run_id,created_at,document) VALUES(?,?,?,?,?,?)")
            .bind(&id).bind(account).bind(text(&run, "session_id")?).bind(run_id).bind(&now).bind(pending.to_string()).execute(&mut *transaction).await?;
        sqlx::query(
            "INSERT INTO messages(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)",
        )
        .bind(&message_id)
        .bind(account)
        .bind(text(&run, "session_id")?)
        .bind(&now)
        .bind(message.to_string())
        .execute(&mut *transaction)
        .await?;
        Self::append_event_in(
            &mut transaction,
            account,
            text(&run, "session_id")?,
            run_id,
            json!({ "type": "CUSTOM", "name": "pixoma.clarification_required", "value": pending }),
        )
        .await?;
        Self::append_event_in(&mut transaction, account, text(&run, "session_id")?, run_id, json!({
            "type": "RUN_FINISHED", "threadId": run["session_id"], "runId": run_id,
            "outcome": { "type": "interrupt", "interrupts": [{ "id": id, "reason": "workflow_input", "message": question, "metadata": pending["metadata"] }] },
        })).await?;
        transaction.commit().await?;
        Ok(true)
    }
}
