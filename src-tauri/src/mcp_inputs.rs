use crate::{
    Error, Result, Store, assets::digest, error::timestamp, local_api::text, tools::RunTool,
};
use rig::message::ToolCall;
use rmcp::model::{
    ElicitRequest, ElicitRequestParams, ElicitResult, ElicitationAction, GetTaskResult,
    InputRequest, InputRequests, InputRequiredResult, TaskPayload,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;

pub(crate) enum McpInput {
    Mrtr(InputRequiredResult),
    Task(GetTaskResult),
    Live(ElicitRequestParams),
}

#[derive(Deserialize, Serialize)]
struct McpChoice {
    id: String,
    label: String,
    value: Option<Value>,
}

#[derive(Deserialize, Serialize)]
struct McpQuestion {
    request_key: String,
    field: Option<String>,
    schema: Option<Value>,
    required: bool,
    message: String,
    choices: Vec<McpChoice>,
}

fn validator(schema: &Value) -> Result<jsonschema::Validator> {
    jsonschema::options()
        .should_validate_formats(true)
        .build(schema)
        .map_err(|_| Error::Invalid("MCP 输入 Schema 无效"))
}

fn field_question(
    key: &str,
    field: &str,
    schema: Value,
    required: bool,
    message: &str,
) -> Result<McpQuestion> {
    let title = schema["title"].as_str().unwrap_or(field);
    let mut question = format!("{message}\n{title}");
    if let Some(description) = schema["description"]
        .as_str()
        .filter(|text| !text.is_empty())
    {
        question.push_str(&format!("\n{description}"));
    }
    if schema["type"] == "array" {
        question.push_str(&format!("\n填写 JSON 数组。可选内容：{}", schema["items"]));
    }
    let mut values = Vec::new();
    if let Some(options) = schema["enum"].as_array() {
        for (index, value) in options.iter().enumerate() {
            let label = schema["enumNames"][index]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string())
                });
            values.push((label, value.clone()));
        }
    } else if let Some(options) = schema["oneOf"].as_array() {
        for option in options {
            let value = option
                .get("const")
                .ok_or(Error::Invalid("MCP 选项缺少内容"))?;
            values.push((
                option["title"]
                    .as_str()
                    .unwrap_or_else(|| value.as_str().unwrap_or(""))
                    .to_owned(),
                value.clone(),
            ));
        }
    } else if schema["type"] == "boolean" {
        values.extend([
            ("是".to_owned(), json!(true)),
            ("否".to_owned(), json!(false)),
        ]);
    } else if schema["type"] == "string" && validator(&schema)?.is_valid(&json!("")) {
        values.push(("空字符串".to_owned(), json!("")));
    }
    if let Some(default) = schema
        .get("default")
        .filter(|value| validator(&schema).is_ok_and(|validator| validator.is_valid(value)))
    {
        values.push((
            format!(
                "使用默认值：{}",
                default
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| default.to_string())
            ),
            default.clone(),
        ));
    }
    let mut choices = values
        .into_iter()
        .enumerate()
        .map(|(index, (label, value))| McpChoice {
            id: format!("value-{index}"),
            label,
            value: Some(value),
        })
        .collect::<Vec<_>>();
    if !required {
        choices.push(McpChoice {
            id: "omit".to_owned(),
            label: "不填写".to_owned(),
            value: None,
        });
    }
    Ok(McpQuestion {
        request_key: key.to_owned(),
        field: Some(field.to_owned()),
        schema: Some(schema),
        required,
        message: question,
        choices,
    })
}

fn advance_input(saved: &mut Value) -> Result<Option<McpQuestion>> {
    let requests: InputRequests = serde_json::from_value(saved["input"]["requests"].clone())?;
    for (key, request) in requests {
        if saved["input"]["responses"].get(&key).is_some() {
            continue;
        }
        let InputRequest::Elicitation(request) = request else {
            return Err(Error::Invalid("MCP 请求了未启用的 sampling 或 roots 能力"));
        };
        match request.params {
            ElicitRequestParams::FormElicitationParams {
                message,
                requested_schema,
                ..
            } => {
                let schema = serde_json::to_value(&requested_schema)?;
                let order: Vec<String> =
                    serde_json::from_value(saved["input"]["orders"][&key].clone())?;
                for field in order {
                    let entered = saved["input"]["values"][&key].get(&field).is_some();
                    let omitted = saved["input"]["omitted"][&key]
                        .as_array()
                        .is_some_and(|fields| fields.contains(&json!(field)));
                    if entered || omitted {
                        continue;
                    }
                    let property = schema["properties"]
                        .get(&field)
                        .ok_or(Error::State("MCP 输入字段缺失"))?;
                    let required = requested_schema
                        .required
                        .as_ref()
                        .is_some_and(|fields| fields.contains(&field));
                    return field_question(&key, &field, property.clone(), required, &message)
                        .map(Some);
                }
                if requested_schema.properties.is_empty()
                    && saved["input"]["confirmed"][&key] != true
                {
                    return Ok(Some(McpQuestion {
                        request_key: key,
                        field: None,
                        schema: None,
                        required: true,
                        message,
                        choices: vec![McpChoice {
                            id: "accept".to_owned(),
                            label: "确认".to_owned(),
                            value: Some(json!(true)),
                        }],
                    }));
                }
                let content = saved["input"]["values"]
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !validator(&schema)?.is_valid(&content) {
                    return Err(Error::Invalid("MCP 输入不符合字段要求"));
                }
                saved["input"]["responses"][&key] = serde_json::to_value(
                    ElicitResult::new(ElicitationAction::Accept).with_content(content),
                )?;
            }
            ElicitRequestParams::UrlElicitationParams { message, url, .. } => {
                let address =
                    url::Url::parse(&url).map_err(|_| Error::Invalid("MCP 补充输入网址无效"))?;
                if !["https", "http"].contains(&address.scheme())
                    || address.host_str().is_none()
                    || !address.username().is_empty()
                    || address.password().is_some()
                {
                    return Err(Error::Invalid("MCP 补充输入网址无效"));
                }
                return Ok(Some(McpQuestion {
                    request_key: key,
                    field: None,
                    schema: None,
                    required: true,
                    message: format!("{message}\n{url}"),
                    choices: vec![McpChoice {
                        id: "accept".to_owned(),
                        label: "已填写".to_owned(),
                        value: Some(json!(true)),
                    }],
                }));
            }
            _ => return Err(Error::Invalid("MCP 补充输入类型不受支持")),
        }
    }
    saved["status"] = json!("ready");
    saved["question"] = Value::Null;
    Ok(None)
}

impl Store {
    pub(crate) async fn close_live_mcp_input(
        &self,
        account: &str,
        run: &str,
        id: &str,
        tool: &RunTool,
        call: &ToolCall,
    ) -> Result<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT m.document,r.document AS run_document FROM mcp_executions m JOIN runs r ON r.account_id=m.account_id AND r.id=m.run_id WHERE m.account_id=? AND m.run_id=? AND m.id=?")
            .bind(account).bind(run).bind(id).fetch_optional(&mut *transaction).await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(());
        };
        let mut saved: Value = serde_json::from_str(row.get("document"))?;
        if saved["binding"] != digest(&serde_json::to_vec(&(tool, call))?) {
            return Err(Error::Invalid("MCP 输入与工具调用不一致"));
        }
        let current: Value = serde_json::from_str(row.get("run_document"))?;
        if saved["input"]["source"] != "live"
            || !matches!(saved["status"].as_str(), Some("input_required" | "ready"))
            || matches!(
                current["status"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            )
        {
            transaction.commit().await?;
            return Ok(());
        }
        if !matches!(
            current["status"].as_str(),
            Some("running" | "waiting_clarification")
        ) {
            return Err(Error::State("MCP 输入等待状态无效"));
        }
        let now = timestamp()?;
        let pending = sqlx::query("SELECT document FROM clarifications WHERE account_id=? AND run_id=? AND json_extract(document,'$.tool_call_id')=? AND json_extract(document,'$.reason')='input_required' AND json_extract(document,'$.status')='pending'")
            .bind(account).bind(run).bind(id).fetch_all(&mut *transaction).await?;
        for row in pending {
            let pending: Value = serde_json::from_str(row.get("document"))?;
            sqlx::query("UPDATE clarifications SET document=json_set(document,'$.status','cancelled','$.updated_at',?) WHERE account_id=? AND run_id=? AND id=?")
                .bind(&now).bind(account).bind(run).bind(text(&pending, "id")?).execute(&mut *transaction).await?;
            sqlx::query("UPDATE messages SET document=json_set(document,'$.clarification.status','skipped') WHERE account_id=? AND id=? AND json_extract(document,'$.clarification.id')=?")
                .bind(account).bind(text(&pending["metadata"], "messageId")?).bind(text(&pending, "id")?).execute(&mut *transaction).await?;
        }
        if current["status"] == "waiting_clarification" {
            sqlx::query("UPDATE runs SET document=json_set(document,'$.status','running','$.updated_at',?,'$.subscription_after_sequence',(SELECT coalesce(max(sequence),0) FROM run_events WHERE run_id=?)) WHERE account_id=? AND id=? AND status='waiting_clarification'")
                .bind(&now).bind(run).bind(account).bind(run).execute(&mut *transaction).await?;
            Self::append_event_in(&mut transaction, account, text(&current, "session_id")?, run, json!({
                "type": "RUN_STARTED", "threadId": current["session_id"], "runId": run, "metadata": { "studioRunId": run },
            })).await?;
        }
        saved["status"] = json!("continuing");
        saved["question"] = Value::Null;
        saved["input"]["status"] = json!("closed");
        sqlx::query("UPDATE mcp_executions SET document=?,updated_at=? WHERE account_id=? AND run_id=? AND id=?")
            .bind(saved.to_string()).bind(&now).bind(account).bind(run).bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.notify_run_events();
        Ok(())
    }

    pub(crate) async fn save_mcp_inputs(
        &self,
        account: &str,
        run: &str,
        id: &str,
        tool: &RunTool,
        call: &ToolCall,
        input: &McpInput,
    ) -> Result<()> {
        let mut saved = self.mcp_execution(account, run, id, tool, call).await?
            .unwrap_or_else(|| json!({ "binding": digest(&serde_json::to_vec(&(tool, call)).expect("工具调用无法编码")) }));
        let (source, requests, response) = match input {
            McpInput::Mrtr(result) => {
                let round = saved["mrtr_round"].as_u64().unwrap_or(0) + 1;
                if round > rmcp::model::DEFAULT_MRTR_MAX_ROUNDS as u64 {
                    return Err(Error::Invalid("MCP 补充输入次数超过限制"));
                }
                saved["mrtr_round"] = json!(round);
                (
                    "mrtr",
                    result.input_requests.clone().unwrap_or_default(),
                    serde_json::to_value(result)?,
                )
            }
            McpInput::Task(result) => {
                if saved["task"]["taskId"] != result.task.task.task_id {
                    return Err(Error::Invalid("MCP 任务编号与输入请求不一致"));
                }
                let TaskPayload::InputRequired { input_requests } = &result.task.payload else {
                    return Err(Error::Invalid("MCP 任务没有等待输入"));
                };
                for (key, request) in input_requests {
                    let request = serde_json::to_value(request)?;
                    if let Some(previous) = saved["task_requests"].get(key)
                        && *previous != request
                    {
                        return Err(Error::Invalid("MCP 任务重复使用了其他输入要求的编号"));
                    }
                    saved["task_requests"][key] = request;
                }
                saved["task"] = serde_json::to_value(&result.task.task)?;
                saved["last_response"] = serde_json::to_value(result)?;
                (
                    "task",
                    input_requests.clone(),
                    serde_json::to_value(result)?,
                )
            }
            McpInput::Live(params) => {
                let requests = std::collections::BTreeMap::from([(
                    "elicitation".to_owned(),
                    InputRequest::Elicitation(ElicitRequest::new(params.clone())),
                )]);
                ("live", requests, serde_json::to_value(params)?)
            }
        };
        let mut orders = serde_json::Map::new();
        for (key, request) in &requests {
            let InputRequest::Elicitation(request) = request else {
                return Err(Error::Invalid("MCP 请求了未启用的 sampling 或 roots 能力"));
            };
            if let ElicitRequestParams::FormElicitationParams {
                requested_schema, ..
            } = &request.params
            {
                let schema = serde_json::to_value(requested_schema)?;
                validator(&schema)?;
                orders.insert(
                    key.clone(),
                    json!(
                        requested_schema
                            .property_order
                            .clone()
                            .unwrap_or_else(|| requested_schema
                                .properties
                                .keys()
                                .cloned()
                                .collect())
                    ),
                );
            }
        }
        if let Some(previous) = saved.get("input").cloned()
            && (source != "task" || previous["requests"] != serde_json::to_value(&requests)?)
        {
            saved
                .as_object_mut()
                .ok_or(Error::State("MCP 执行记录无效"))?
                .entry("input_history")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or(Error::State("MCP 输入历史无效"))?
                .push(previous);
        }
        let responses = if source == "task" {
            saved
                .get("task_responses")
                .cloned()
                .unwrap_or_else(|| json!({}))
        } else {
            json!({})
        };
        saved["input"] = json!({ "source": source, "response": response, "requests": requests, "orders": orders,
            "responses": responses, "values": {}, "omitted": {}, "confirmed": {} });
        saved["status"] = json!("input_required");
        saved["question"] = Value::Null;
        let now = timestamp()?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query("UPDATE tool_calls SET status=?,updated_at=? WHERE account_id=? AND run_id=? AND id=? AND status IN ('executing','waiting_mcp') AND EXISTS(SELECT 1 FROM runs WHERE runs.account_id=tool_calls.account_id AND runs.id=tool_calls.run_id AND runs.status='running')")
            .bind(if source == "live" { "executing" } else { "waiting_mcp" }).bind(&now).bind(account).bind(run).bind(id).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::State("MCP 工具无法等待输入"));
        }
        sqlx::query("INSERT INTO mcp_executions(id,account_id,run_id,document,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(run_id,id) DO UPDATE SET document=excluded.document,updated_at=excluded.updated_at")
            .bind(id).bind(account).bind(run).bind(saved.to_string()).bind(&now).bind(&now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn pause_for_mcp_input(
        &self,
        account: &str,
        run: &str,
        id: &str,
        tool: &RunTool,
        call: &ToolCall,
    ) -> Result<bool> {
        let mut saved = self
            .mcp_execution(account, run, id, tool, call)
            .await?
            .ok_or(Error::State("MCP 继续记录缺失"))?;
        let question = advance_input(&mut saved)?;
        let current = self.get_run(account, run).await?;
        let now = timestamp()?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE account_id=? AND id=? AND status='running')",
        )
        .bind(account)
        .bind(run)
        .fetch_one(&mut *transaction)
        .await?;
        if !active {
            return Err(Error::State("运行无法等待 MCP 输入"));
        }
        if let Some(question) = &question {
            saved["question"] = serde_json::to_value(question)?;
            let interrupt = uuid::Uuid::new_v4().to_string();
            let message_id = format!("{id}-mcp-input-{interrupt}");
            let options = question
                .choices
                .iter()
                .map(|choice| json!({ "id": choice.id, "label": choice.label }))
                .collect::<Vec<_>>();
            let pending = json!({ "id": interrupt, "reason": "input_required", "message": question.message, "status": "pending", "tool_call_id": id,
                "binding": saved["binding"], "question": question, "metadata": { "messageId": message_id, "options": options } });
            let message = json!({ "id": message_id, "session_id": current["session_id"], "run_id": run, "role": "assistant", "created_at": now,
                "content": [{ "type": "text", "text": question.message }], "clarification": {
                    "id": interrupt, "question": question.message, "options": options, "status": "pending",
                } });
            sqlx::query("UPDATE runs SET document=json_set(document,'$.status','waiting_clarification','$.updated_at',?) WHERE account_id=? AND id=? AND status='running'")
                .bind(&now).bind(account).bind(run).execute(&mut *transaction).await?;
            sqlx::query("INSERT INTO clarifications(id,account_id,session_id,run_id,created_at,document) VALUES(?,?,?,?,?,?)")
                .bind(&interrupt).bind(account).bind(text(&current, "session_id")?).bind(run).bind(&now).bind(pending.to_string()).execute(&mut *transaction).await?;
            sqlx::query("INSERT INTO messages(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)")
                .bind(&message_id).bind(account).bind(text(&current, "session_id")?).bind(&now).bind(message.to_string()).execute(&mut *transaction).await?;
            Self::append_event_in(&mut transaction, account, text(&current, "session_id")?, run,
                json!({ "type": "CUSTOM", "name": "pixoma.clarification_required", "value": pending })).await?;
            Self::append_event_in(&mut transaction, account, text(&current, "session_id")?, run, json!({
                "type": "RUN_FINISHED", "threadId": current["session_id"], "runId": run,
                "outcome": { "type": "interrupt", "interrupts": [{ "id": interrupt, "reason": "input_required", "message": question.message, "metadata": pending["metadata"] }] },
            })).await?;
        }
        sqlx::query("UPDATE mcp_executions SET document=?,updated_at=? WHERE account_id=? AND run_id=? AND id=?")
            .bind(saved.to_string()).bind(&now).bind(account).bind(run).bind(id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.notify_run_events();
        Ok(question.is_some())
    }

    pub(crate) async fn resolve_mcp_input(
        &self,
        account: &str,
        session: &str,
        id: &str,
        response: &Value,
    ) -> Result<Value> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT c.document,c.run_id,t.id AS call_id,t.call,t.definition,t.approved_arguments,t.status AS tool_status,m.document AS execution FROM clarifications c JOIN tool_calls t ON t.run_id=c.run_id AND t.account_id=c.account_id AND t.id=json_extract(c.document,'$.tool_call_id') JOIN mcp_executions m ON m.account_id=t.account_id AND m.run_id=t.run_id AND m.id=t.id WHERE c.account_id=? AND c.session_id=? AND c.id=? AND json_extract(c.document,'$.reason')='input_required'")
            .bind(account).bind(session).bind(id).fetch_optional(&mut *transaction).await?.ok_or(Error::NotFound)?;
        let mut pending: Value = serde_json::from_str(row.get("document"))?;
        let run: String = row.get("run_id");
        if pending["status"] != "pending" {
            if pending["response"] != *response {
                return Err(Error::Invalid("MCP 输入已经收到其他答复"));
            }
            transaction.commit().await?;
            return self.get_run(account, &run).await;
        }
        let mut saved: Value = serde_json::from_str(row.get("execution"))?;
        let mut call: ToolCall = serde_json::from_str(row.get("call"))?;
        if let Some(arguments) = row.get::<Option<String>, _>("approved_arguments") {
            call.function.arguments = serde_json::from_str(&arguments)?;
        }
        let tool: RunTool = serde_json::from_str(
            &row.get::<Option<String>, _>("definition")
                .ok_or(Error::State("MCP 工具定义缺失"))?,
        )?;
        if pending["binding"] != saved["binding"]
            || saved["binding"] != digest(&serde_json::to_vec(&(&tool, &call))?)
            || pending["question"] != saved["question"]
        {
            return Err(Error::Invalid("MCP 输入与工具调用不一致"));
        }
        let live = saved["input"]["source"] == "live";
        if row.get::<String, _>("tool_status") != if live { "executing" } else { "waiting_mcp" } {
            return Err(Error::Invalid("MCP 工具调用已经结束"));
        }
        let question: McpQuestion = serde_json::from_value(saved["question"].clone())?;
        let key = &question.request_key;
        let status = text(response, "status")?;
        let selected = if status == "cancelled" {
            "cancel"
        } else if status == "resolved" {
            text(&response["payload"], "selected")?
        } else {
            return Err(Error::Invalid("MCP 输入答复无效"));
        };
        let mut answer = String::new();
        if selected == "cancel" || (selected == "skip" && question.required) {
            saved["input"]["responses"][key] =
                serde_json::to_value(ElicitResult::new(if selected == "cancel" {
                    ElicitationAction::Cancel
                } else {
                    ElicitationAction::Decline
                }))?;
        } else if selected == "skip" || selected == "omit" {
            if question.required {
                return Err(Error::Invalid("MCP 必填字段不能省略"));
            }
            let omitted = saved["input"]["omitted"]
                .as_object_mut()
                .ok_or(Error::State("MCP 输入记录无效"))?
                .entry(key.clone())
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or(Error::State("MCP 省略字段记录无效"))?;
            omitted.push(json!(
                question
                    .field
                    .as_ref()
                    .ok_or(Error::State("MCP 输入字段缺失"))?
            ));
        } else {
            let value = if selected == "other" {
                let custom = text(&response["payload"], "custom")?;
                if custom.len() > 8 << 20 {
                    return Err(Error::Invalid("MCP 输入超过 8 MiB"));
                }
                answer = custom.to_owned();
                match question
                    .schema
                    .as_ref()
                    .and_then(|schema| schema["type"].as_str())
                {
                    Some("string") => json!(custom),
                    Some(_) => serde_json::from_str(custom)
                        .map_err(|_| Error::Invalid("MCP 输入需要有效的 JSON 内容"))?,
                    None => return Err(Error::Invalid("MCP 输入需要选择确认选项")),
                }
            } else {
                let choice = question
                    .choices
                    .iter()
                    .find(|choice| choice.id == selected)
                    .ok_or(Error::Invalid("MCP 输入选项无效"))?;
                answer = choice.label.clone();
                choice
                    .value
                    .clone()
                    .ok_or(Error::Invalid("MCP 输入选项缺少内容"))?
            };
            if let Some(field) = &question.field {
                if !validator(
                    question
                        .schema
                        .as_ref()
                        .ok_or(Error::State("MCP 字段 Schema 缺失"))?,
                )?
                .is_valid(&value)
                {
                    return Err(Error::Invalid("MCP 输入不符合字段要求"));
                }
                saved["input"]["values"][key][field] = value;
            } else {
                let requests: InputRequests =
                    serde_json::from_value(saved["input"]["requests"].clone())?;
                let Some(InputRequest::Elicitation(request)) = requests.get(key) else {
                    return Err(Error::State("MCP 输入请求缺失"));
                };
                match &request.params {
                    ElicitRequestParams::UrlElicitationParams { .. } => {
                        saved["input"]["responses"][key] =
                            serde_json::to_value(ElicitResult::new(ElicitationAction::Accept))?
                    }
                    ElicitRequestParams::FormElicitationParams { .. } => {
                        saved["input"]["confirmed"][key] = json!(true)
                    }
                    _ => return Err(Error::Invalid("MCP 补充输入类型不受支持")),
                }
            }
        }
        saved["question"] = Value::Null;
        let now = timestamp()?;
        let changed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status',?,'$.updated_at',?,'$.subscription_after_sequence',(SELECT coalesce(max(sequence),0) FROM run_events WHERE run_id=?)) WHERE account_id=? AND id=? AND status='waiting_clarification'")
            .bind(if live { "running" } else { "queued" }).bind(&now).bind(&run).bind(account).bind(&run).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Invalid("运行已经结束或输入状态已更改"));
        }
        let status = if ["skip", "cancel", "omit"].contains(&selected) {
            "skipped"
        } else {
            "answered"
        };
        pending["status"] = json!(status);
        pending["response"] = response.clone();
        pending["updated_at"] = json!(now);
        sqlx::query("UPDATE clarifications SET document=? WHERE account_id=? AND id=?")
            .bind(pending.to_string())
            .bind(account)
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE messages SET document=json_set(document,'$.clarification.status',?,'$.clarification.selected',?,'$.clarification.answer',?) WHERE account_id=? AND id=? AND json_extract(document,'$.clarification.id')=?")
            .bind(status).bind(selected).bind(answer).bind(account).bind(text(&pending["metadata"], "messageId")?).bind(id).execute(&mut *transaction).await?;
        sqlx::query("UPDATE mcp_executions SET document=?,updated_at=? WHERE account_id=? AND run_id=? AND id=?")
            .bind(saved.to_string()).bind(&now).bind(account).bind(&run).bind(row.get::<String, _>("call_id")).execute(&mut *transaction).await?;
        if live {
            Self::append_event_in(&mut transaction, account, session, &run, json!({
                "type": "RUN_STARTED", "threadId": session, "runId": run, "metadata": { "studioRunId": run },
            })).await?;
        }
        transaction.commit().await?;
        self.notify_run_events();
        self.get_run(account, &run).await
    }
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
    use rmcp::model::InputRequiredResult;
    use serde_json::{Value, json};

    #[tokio::test]
    #[ignore = "需要 PIXOMA_TEST_ELICIT_MCP_URL 并连接官方 SDK enum form 服务"]
    async fn real_sdk_enum_form_updates_from_the_submitted_answers()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("mcp-elicitation-live-")
            .tempdir_in(root)?;
        let database = directory.path().join("studio.sqlite");
        let mut store = Store::open(&database).await?;
        let account = "mcp-elicitation-live";
        let model = store.save_model(account, None, json!({
            "name": "补充输入调用检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:18082/v1", "model": "input-state",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
        })).await?;
        store.save_connector(account, None, json!({
            "name": "官方 SDK 输入服务", "url": std::env::var("PIXOMA_TEST_ELICIT_MCP_URL")?, "credential": "", "enabled": true, "policy": "auto",
        })).await?;
        let tool = store
            .mcp_run_tools(account)
            .await?
            .into_iter()
            .find(|tool| tool.display_name() == "set_enum_form")
            .ok_or("表单工具缺失")?;
        let session = store.create_session(account, "elicitation-live").await?;
        let queued = store
            .queue_run(
                account,
                &session.id,
                "elicitation-live",
                text(&model, "id")?,
                "填写选项",
                "full_access",
            )
            .await?;
        let run = text(&queued["run"], "id")?.to_owned();
        assert!(store.claim_run(account, &run).await?);
        let call = ToolCall::from_wire(
            "elicitation-live",
            ToolFunction::new(tool.definition.name.clone(), json!({})),
        );
        store.save_tool_part(account, &run, 1, 0, &call).await?;
        let record = store.tool_call_records(account, &run, 1).await?.remove(0);
        let id = text(&record, "id")?.to_owned();
        let runtime = crate::AgentRuntime::new(store.clone());
        let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let events = received.clone();
        let channel = tauri::ipc::Channel::new(move |body| {
            let tauri::ipc::InvokeResponseBody::Json(body) = body else {
                panic!("事件需要 JSON 编码");
            };
            events
                .lock()
                .expect("事件无法保存")
                .push(serde_json::from_str(&body).expect("事件格式无效"));
            Ok(())
        });
        let event_run = run.clone();
        let subscription = tokio::spawn(async move {
            runtime
                .forward_events(
                    account,
                    &event_run,
                    0,
                    channel,
                    tokio_util::sync::CancellationToken::new(),
                )
                .await
        });
        let worker_store = store.clone();
        let worker_run = run.clone();
        let worker_tool = tool.clone();
        let worker_call = call.clone();
        let worker = tokio::spawn(async move {
            worker_store
                .execute_local_tool(account, &worker_run, &record, &worker_tool, &worker_call)
                .await
        });
        let mut answered = std::collections::BTreeSet::new();
        for _ in 0..4 {
            let pending = tokio::time::timeout(std::time::Duration::from_secs(15), async {
                loop {
                    let pending: Option<String> = sqlx::query_scalar("SELECT document FROM clarifications WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending'")
                        .bind(account).bind(&run).fetch_optional(&store.pool).await?;
                    if let Some(pending) = pending {
                        let pending: Value = serde_json::from_str(&pending)?;
                        if !answered.contains(text(&pending, "id")?) {
                            let saved = store
                                .mcp_execution(account, &run, &id, &tool, &call)
                                .await?
                                .ok_or(crate::Error::State("MCP 输入记录缺失"))?;
                            return Ok::<_, crate::Error>((pending, saved));
                        }
                    }
                    if worker.is_finished() {
                        return Err(crate::Error::State("MCP 调用提前结束"));
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            })
            .await;
            let (pending, saved) = match pending {
                Ok(result) => result?,
                Err(error) => {
                    let current = store.get_run(account, &run).await?;
                    let execution = store
                        .mcp_execution(account, &run, &id, &tool, &call)
                        .await?;
                    return Err(format!("等待第 {} 个 MCP 输入超时：{error}；运行状态={}；调用完成={}；输入记录={execution:?}", answered.len() + 1, current["status"], worker.is_finished()).into());
                }
            };
            if answered.is_empty() {
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    while !subscription.is_finished() {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                })
                .await?;
                {
                    let events = received.lock().map_err(|_| "事件无法读取")?;
                    assert!(
                        events
                            .iter()
                            .any(|event| event["name"] == "pixoma.clarification_required")
                    );
                    assert_eq!(
                        events.last().ok_or("结束事件缺失")?["outcome"]["interrupts"][0]["reason"],
                        "input_required"
                    );
                }
                assert!(!worker.is_finished());
                let wait = match std::env::var("PIXOMA_TEST_ELICIT_WAIT_SECONDS") {
                    Ok(value) => value.parse::<u64>()?,
                    Err(std::env::VarError::NotPresent) => 0,
                    Err(error) => return Err(error.into()),
                };
                if wait > 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    assert!(!worker.is_finished());
                    assert_eq!(
                        store.get_run(account, &run).await?["status"],
                        "waiting_clarification"
                    );
                }
            }
            let custom = match text(&saved["question"], "field")? {
                "single_select_untitled" => "Second",
                "single_select_titled" => "SecondValue",
                "multi_select_untitled" => "[\"First\",\"Third\"]",
                "multi_select_titled" => "[\"FirstValue\",\"SecondValue\"]",
                _ => return Err("服务返回了未预期的字段".into()),
            };
            let answer = json!({ "interruptId": pending["id"], "status": "resolved", "payload": { "selected": "other", "custom": custom } });
            store
                .resolve_run_interrupt(account, &session.id, &answer)
                .await?;
            answered.insert(text(&pending, "id")?.to_owned());
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(15), worker)
            .await
            .map_err(|error| {
                format!(
                    "MCP 已收到 {} 项输入，等待工具结果超时：{error}",
                    answered.len()
                )
            })???
            .ok_or("工具没有返回结果")?;
        assert!(
            matches!(result, rig::message::UserContent::ToolResult(result) if !result.is_error)
        );
        subscription.await??;
        let record = store.tool_call_records(account, &run, 1).await?.remove(0);
        assert_eq!(record["status"], "succeeded");
        let raw: String = sqlx::query_scalar(
            "SELECT raw_result FROM tool_calls WHERE account_id=? AND run_id=? AND id=?",
        )
        .bind(account)
        .bind(&run)
        .bind(&id)
        .fetch_one(&store.pool)
        .await?;
        let raw: Value = serde_json::from_str(&raw)?;
        let content = text(&raw["content"][0], "text")?;
        for expected in [
            "Single Select Untitled: Second",
            "Single Select Titled: SecondValue",
            "Multi Select Untitled: [First, Third]",
            "Multi Select Titled: [FirstValue, SecondValue]",
        ] {
            assert!(content.contains(expected), "{content}");
        }
        store.finish_run(account, &run, "succeeded", None).await?;
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 0);
        let saved = store
            .mcp_execution(account, &run, &id, &tool, &call)
            .await?
            .ok_or("原始响应缺失")?;
        assert_eq!(saved["response"], raw);
        assert_eq!(
            saved["input"]["responses"]["elicitation"]["content"]["multi_select_untitled"],
            json!(["First", "Third"])
        );
        store.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn form_answers_keep_the_opaque_state_and_validate_each_field_after_reopening()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("mcp-inputs-")
            .tempdir_in(root)?;
        let database = directory.path().join("studio.sqlite");
        let mut store = Store::open(&database).await?;
        let account = "mcp-inputs";
        let model = store.save_model(account, None, json!({
            "name": "补充输入检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:18082/v1", "model": "input-state",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
        })).await?;
        let session = store.create_session(account, "inputs").await?;
        let queued = store
            .queue_run(
                account,
                &session.id,
                "inputs",
                text(&model, "id")?,
                "填写字段",
                "full_access",
            )
            .await?;
        let run = text(&queued["run"], "id")?;
        assert!(store.claim_run(account, run).await?);
        let tool = RunTool {
            definition: ToolDefinition::new(
                ToolName::new("write_contact")?,
                "保存联系方式",
                json!({ "type": "object" }),
            ),
            target: ToolTarget::Mcp {
                connector_id: "input-check".to_owned(),
                url: "http://127.0.0.1:18082/mcp".to_owned(),
                name: "write_contact".to_owned(),
                credential_ref: None,
            },
            requires_approval: true,
            read_only: false,
        };
        let call = ToolCall::from_wire(
            "input-check",
            ToolFunction::new(tool.definition.name.clone(), json!({})),
        );
        store.save_tool_part(account, run, 1, 0, &call).await?;
        let record = store.tool_call_records(account, run, 1).await?.remove(0);
        let id = text(&record, "id")?;
        sqlx::query("UPDATE tool_calls SET status='executing',definition=? WHERE account_id=? AND run_id=? AND id=?")
            .bind(serde_json::to_string(&tool)?).bind(account).bind(run).bind(id).execute(&store.pool).await?;
        let opaque = "原样保存的状态\0{未解析}";
        let mut input: InputRequiredResult = serde_json::from_value(json!({
            "resultType": "input_required", "requestState": opaque, "_meta": { "trace": "form" },
            "inputRequests": { "contact": { "method": "elicitation/create", "params": {
                "mode": "form", "message": "填写联系方式", "requestedSchema": {
                    "type": "object", "properties": {
                        "email": { "type": "string", "format": "email", "title": "电子邮箱" },
                        "count": { "type": "integer", "minimum": 1, "maximum": 4 },
                        "labels": { "type": "array", "items": { "type": "string", "enum": ["work", "home"] }, "minItems": 1, "maxItems": 2 },
                        "note": { "type": "string" }
                    }, "required": ["email", "count", "labels"]
                }
            } } }
        }))?;
        if let Some(rmcp::model::InputRequest::Elicitation(request)) = input
            .input_requests
            .as_mut()
            .and_then(|requests| requests.get_mut("contact"))
            && let rmcp::model::ElicitRequestParams::FormElicitationParams {
                requested_schema, ..
            } = &mut request.params
        {
            requested_schema.property_order = Some(vec![
                "email".to_owned(),
                "count".to_owned(),
                "labels".to_owned(),
                "note".to_owned(),
            ]);
        }
        store
            .save_mcp_inputs(
                account,
                run,
                id,
                &tool,
                &call,
                &super::McpInput::Mrtr(input),
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
        let pending: String = sqlx::query_scalar("SELECT document FROM clarifications WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending'")
            .bind(account).bind(run).fetch_one(&store.pool).await?;
        let pending: Value = serde_json::from_str(&pending)?;
        let answer = json!({ "interruptId": pending["id"], "status": "resolved", "payload": { "selected": "other", "custom": "bad-email" } });
        assert!(
            store
                .resolve_run_interrupt(account, &session.id, &answer)
                .await
                .is_err()
        );
        let answer = json!({ "interruptId": pending["id"], "status": "resolved", "payload": { "selected": "other", "custom": "input@example.com" } });
        assert!(
            store
                .resolve_run_interrupt("another-account", &session.id, &answer)
                .await
                .is_err()
        );
        let (first, second) = tokio::join!(
            store.resolve_run_interrupt(account, &session.id, &answer),
            store.resolve_run_interrupt(account, &session.id, &answer)
        );
        assert_eq!(first?["id"], run);
        assert_eq!(second?["id"], run);
        for (field, custom) in [
            ("count", Some("2")),
            ("labels", Some("[\"work\",\"home\"]")),
            ("note", None),
        ] {
            assert!(store.claim_run(account, run).await?);
            assert!(
                store
                    .pause_for_mcp_input(account, run, id, &tool, &call)
                    .await?
            );
            let saved = store
                .mcp_execution(account, run, id, &tool, &call)
                .await?
                .ok_or("继续记录缺失")?;
            assert_eq!(saved["question"]["field"], field);
            let pending: String = sqlx::query_scalar("SELECT document FROM clarifications WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending'")
                .bind(account).bind(run).fetch_one(&store.pool).await?;
            let pending: Value = serde_json::from_str(&pending)?;
            let answer = json!({ "interruptId": pending["id"], "status": "resolved", "payload": {
                "selected": if custom.is_some() { "other" } else { "omit" }, "custom": custom,
            } });
            store
                .resolve_run_interrupt(account, &session.id, &answer)
                .await?;
        }
        assert!(store.claim_run(account, run).await?);
        assert!(
            !store
                .pause_for_mcp_input(account, run, id, &tool, &call)
                .await?
        );
        let saved = store
            .mcp_execution(account, run, id, &tool, &call)
            .await?
            .ok_or("继续记录缺失")?;
        assert_eq!(saved["status"], "ready");
        assert_eq!(saved["input"]["response"]["requestState"], opaque);
        assert_eq!(saved["input"]["response"]["_meta"]["trace"], "form");
        assert_eq!(
            saved["input"]["responses"]["contact"],
            json!({ "action": "accept", "content": {
            "email": "input@example.com", "count": 2, "labels": ["work", "home"],
        } })
        );
        assert_eq!(
            store.tool_call_records(account, run, 1).await?[0]["status"],
            "waiting_mcp"
        );
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 1);
        assert_eq!(store.get_run(account, run).await?["status"], "queued");
        assert!(store.claim_run(account, run).await?);
        let request = store
            .begin_mcp_continuation(account, run, id, &tool, &call)
            .await?;
        assert_eq!(request.request_state.as_deref(), Some(opaque));
        assert_eq!(request.name.as_ref(), "write_contact");
        assert_eq!(request.arguments, Some(call.function.arguments.clone()));
        assert_eq!(
            request.input_responses.ok_or("输入答复缺失")?["contact"]["content"]["count"],
            2
        );
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 1);
        assert_eq!(
            store.get_run(account, run).await?["error_code"],
            "tool_result_unknown"
        );
        assert_eq!(
            store.tool_call_records(account, run, 1).await?[0]["status"],
            "result_unknown"
        );
        assert!(
            store
                .begin_mcp_continuation(account, run, id, &tool, &call)
                .await
                .is_err()
        );
        let queued = store
            .queue_run(
                account,
                &session.id,
                "live-inputs",
                text(&model, "id")?,
                "填写联系信息",
                "full_access",
            )
            .await?;
        let live_run = text(&queued["run"], "id")?;
        assert!(store.claim_run(account, live_run).await?);
        let live_call = ToolCall::from_wire(
            "live-input",
            ToolFunction::new(tool.definition.name.clone(), json!({})),
        );
        store
            .save_tool_part(account, live_run, 1, 0, &live_call)
            .await?;
        let record = store
            .tool_call_records(account, live_run, 1)
            .await?
            .remove(0);
        let live_id = text(&record, "id")?;
        sqlx::query("UPDATE tool_calls SET status='executing',definition=? WHERE account_id=? AND run_id=? AND id=?")
            .bind(serde_json::to_string(&tool)?).bind(account).bind(live_run).bind(live_id).execute(&store.pool).await?;
        let input = rmcp::model::ElicitRequestParams::FormElicitationParams {
            meta: None,
            message: "确认联系信息".to_owned(),
            requested_schema: serde_json::from_value(
                json!({ "type": "object", "properties": { "confirm": { "type": "boolean" }, "note": { "type": "string" } }, "required": ["confirm", "note"] }),
            )?,
        };
        store
            .save_mcp_inputs(
                account,
                live_run,
                live_id,
                &tool,
                &live_call,
                &super::McpInput::Live(input.clone()),
            )
            .await?;
        assert!(
            store
                .pause_for_mcp_input(account, live_run, live_id, &tool, &live_call)
                .await?
        );
        let pending: String = sqlx::query_scalar("SELECT document FROM clarifications WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending'")
            .bind(account).bind(live_run).fetch_one(&store.pool).await?;
        let pending: Value = serde_json::from_str(&pending)?;
        let before = store
            .run_events_after(account, live_run, 0)
            .await?
            .last()
            .ok_or("输入事件缺失")?["sequence"]
            .as_i64()
            .ok_or("输入事件序号缺失")?;
        let answer = json!({ "interruptId": pending["id"], "status": "resolved", "payload": { "selected": "value-0" } });
        assert_eq!(
            store
                .resolve_run_interrupt(account, &session.id, &answer)
                .await?["status"],
            "running"
        );
        assert_eq!(
            store
                .run_events_after(account, live_run, before)
                .await?
                .first()
                .and_then(|event| event["type"].as_str()),
            Some("RUN_STARTED")
        );
        assert!(
            store
                .pause_for_mcp_input(account, live_run, live_id, &tool, &live_call)
                .await?
        );
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 1);
        assert_eq!(
            store.get_run(account, live_run).await?["error_code"],
            "tool_result_unknown"
        );
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clarifications WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending')")
            .bind(account).bind(live_run).fetch_one(&store.pool).await?;
        assert!(!pending);
        assert_eq!(
            store.tool_call_records(account, live_run, 1).await?[0]["status"],
            "result_unknown"
        );
        let queued = store
            .queue_run(
                account,
                &session.id,
                "withdrawn-input",
                text(&model, "id")?,
                "确认联系信息",
                "full_access",
            )
            .await?;
        let withdrawn_run = text(&queued["run"], "id")?;
        assert!(store.claim_run(account, withdrawn_run).await?);
        store
            .save_tool_part(account, withdrawn_run, 1, 0, &live_call)
            .await?;
        let record = store
            .tool_call_records(account, withdrawn_run, 1)
            .await?
            .remove(0);
        let withdrawn_id = text(&record, "id")?;
        sqlx::query("UPDATE tool_calls SET status='executing',definition=? WHERE account_id=? AND run_id=? AND id=?")
            .bind(serde_json::to_string(&tool)?).bind(account).bind(withdrawn_run).bind(withdrawn_id).execute(&store.pool).await?;
        store
            .save_mcp_inputs(
                account,
                withdrawn_run,
                withdrawn_id,
                &tool,
                &live_call,
                &super::McpInput::Live(input),
            )
            .await?;
        assert!(
            store
                .pause_for_mcp_input(account, withdrawn_run, withdrawn_id, &tool, &live_call)
                .await?
        );
        let result = rmcp::model::CallToolResult::success(Vec::new());
        assert!(
            store
                .save_mcp_completion(
                    account,
                    withdrawn_run,
                    withdrawn_id,
                    &tool,
                    &live_call,
                    &result
                )
                .await
                .is_err()
        );
        store
            .close_live_mcp_input(account, withdrawn_run, withdrawn_id, &tool, &live_call)
            .await?;
        assert_eq!(
            store.get_run(account, withdrawn_run).await?["status"],
            "running"
        );
        let event_count = store
            .run_events_after(account, withdrawn_run, 0)
            .await?
            .len();
        store
            .close_live_mcp_input(account, withdrawn_run, withdrawn_id, &tool, &live_call)
            .await?;
        assert_eq!(
            store
                .run_events_after(account, withdrawn_run, 0)
                .await?
                .len(),
            event_count
        );
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clarifications WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending')")
            .bind(account).bind(withdrawn_run).fetch_one(&store.pool).await?;
        assert!(!pending);
        store
            .save_mcp_completion(
                account,
                withdrawn_run,
                withdrawn_id,
                &tool,
                &live_call,
                &result,
            )
            .await?;
        let saved = store
            .mcp_execution(account, withdrawn_run, withdrawn_id, &tool, &live_call)
            .await?
            .ok_or("MCP 完整结果缺失")?;
        assert_eq!(saved["response"], serde_json::to_value(result)?);
        assert_eq!(saved["input"]["status"], "closed");
        assert!(
            store
                .run_events_after(account, withdrawn_run, 0)
                .await?
                .iter()
                .any(|event| event["type"] == "RUN_STARTED")
        );
        store
            .finish_run(account, withdrawn_run, "succeeded", None)
            .await?;
        store.close().await;
        Ok(())
    }
}
