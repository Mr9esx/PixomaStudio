use crate::{
    Error, Result, Store,
    error::timestamp,
    local_api::{optional_text, text},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExpertCardResponse {
    response_id: String,
    action: String,
    #[serde(default)]
    inputs: Map<String, Value>,
    #[serde(default)]
    comment: String,
}

pub(crate) struct ExpertCardDecision {
    pub card_id: String,
    pub task_id: String,
    pub response: ExpertCardResponse,
}

fn validate_inputs(card: &Value, inputs: &Value, require_values: bool) -> Result<()> {
    let fields = match card.get("fields") {
        Some(fields) => fields
            .as_array()
            .ok_or(Error::Invalid("步骤字段格式无效"))?
            .clone(),
        None => Vec::new(),
    };
    let mut properties = Map::new();
    let mut required = Vec::new();
    for field in &fields {
        let key = text(field, "key")?;
        let mandatory = require_values && field["required"] == true;
        let mut schema = match text(field, "type")? {
            "text" | "textarea" => json!({ "type": "string", "minLength": usize::from(mandatory) }),
            "number" => json!({ "type": "number" }),
            "boolean" => json!({ "type": "boolean" }),
            "select" => {
                let mut options = field["options"]
                    .as_array()
                    .ok_or(Error::Invalid("选择字段缺少选项"))?
                    .clone();
                if !mandatory {
                    options.push(json!(""));
                }
                json!({ "type": "string", "enum": options })
            }
            "asset" | "assets" => {
                let reference = json!({ "type": "object", "additionalProperties": false, "properties": {
                    "asset_id": { "type": "string", "minLength": 1 }, "asset_version_id": { "type": "string", "minLength": 1 },
                }, "required": ["asset_id", "asset_version_id"] });
                if field["type"] == "assets" {
                    json!({ "type": "array", "items": reference, "minItems": usize::from(mandatory) })
                } else {
                    reference
                }
            }
            _ => return Err(Error::Invalid("步骤字段类型无效")),
        };
        if mandatory {
            required.push(json!(key));
            if inputs[key]
                .as_str()
                .is_some_and(|value| value.trim().is_empty())
            {
                return Err(Error::Invalid("请填写必填的步骤输入"));
            }
        } else {
            schema = json!({ "anyOf": [schema, { "type": "null" }] });
        }
        properties.insert(key.to_owned(), schema);
    }
    let schema = json!({ "type": "object", "additionalProperties": false, "properties": properties, "required": required });
    let validator =
        jsonschema::validator_for(&schema).map_err(|_| Error::State("步骤输入 Schema 无效"))?;
    if !validator.is_valid(inputs) {
        return Err(Error::Invalid("步骤输入缺少内容或类型无效"));
    }
    Ok(())
}

fn input_assets(card: &Value, inputs: &Value) -> Result<Vec<Value>> {
    let mut references = Vec::new();
    for field in card
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let key = text(field, "key")?;
        if inputs[key].is_null() {
            continue;
        }
        match text(field, "type")? {
            "asset" => references.push(inputs[key].clone()),
            "assets" => references.extend(
                inputs[key]
                    .as_array()
                    .ok_or(Error::Invalid("步骤资产输入需要数组"))?
                    .iter()
                    .cloned(),
            ),
            _ => {}
        }
    }
    Ok(references)
}

impl Store {
    async fn expert_card(&self, account: &str, id: &str) -> Result<Value> {
        Self::check_account(account)?;
        let document: String =
            sqlx::query_scalar("SELECT document FROM expert_cards WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Error::NotFound)?;
        Ok(serde_json::from_str(&document)?)
    }

    pub(crate) async fn create_expert_card(
        &self,
        account: &str,
        run: &Value,
        call_id: &str,
        input: &Value,
    ) -> Result<Value> {
        let mut task = self.current_expert_task(account, run).await?;
        if ["completed", "cancelled", "failed"].contains(&text(&task, "status")?) {
            return Err(Error::Invalid("专家任务已经结束"));
        }
        let step_id = text(input, "step_id")?;
        let index = task["steps"]
            .as_array()
            .ok_or(Error::State("专家步骤格式无效"))?
            .iter()
            .position(|step| step["step_id"] == step_id)
            .ok_or(Error::Invalid("专家步骤不存在"))?;
        let content = input
            .get("card")
            .ok_or(Error::Invalid("专家卡片内容缺失"))?;
        let mode = text(content, "mode")?;
        if !["step_entry", "result_review"].contains(&mode) {
            return Err(Error::Invalid("专家卡片类型无效"));
        }
        let reason = text(content, "reason")?;
        if reason.trim().is_empty() || reason.chars().count() > 2000 {
            return Err(Error::Invalid("专家卡片说明无效"));
        }
        let mut keys = HashSet::new();
        for field in content
            .get("fields")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if !keys.insert(text(field, "key")?) {
                return Err(Error::Invalid("专家卡片字段标识重复"));
            }
            if field["type"] == "select"
                && field["options"].as_array().is_none_or(|options| {
                    options.is_empty()
                        || options
                            .iter()
                            .any(|option| option.as_str().is_none_or(str::is_empty))
                })
            {
                return Err(Error::Invalid("选择字段需要非空选项"));
            }
        }
        let suggested = content
            .get("suggested_inputs")
            .cloned()
            .unwrap_or_else(|| json!({}));
        validate_inputs(content, &suggested, false)?;
        let mut transaction = self.pool.begin().await?;
        Self::validate_expert_result_in(
            &mut transaction,
            account,
            text(run, "session_id")?,
            &json!({ "assets": input_assets(content, &suggested)? }),
            false,
        )
        .await?;
        let attempt_id = optional_text(input, "attempt_id", "")?;
        if mode == "result_review" {
            if attempt_id.is_empty() || task["steps"][index]["latest_attempt_id"] != attempt_id {
                return Err(Error::Invalid("结果卡片需要当前步骤的处理记录"));
            }
            Self::validate_expert_result_in(
                &mut transaction,
                account,
                text(run, "session_id")?,
                content
                    .get("results")
                    .ok_or(Error::Invalid("结果卡片缺少实际结果"))?,
                true,
            )
            .await?;
            Self::validate_expert_attempt_workflows_in(&mut transaction, account, attempt_id)
                .await?;
        }
        if !attempt_id.is_empty() {
            let mut attempt =
                Self::expert_attempt_in(&mut transaction, account, attempt_id).await?;
            if attempt["expert_task_id"] != task["id"]
                || attempt["step_id"] != step_id
                || task["steps"][index]["latest_attempt_id"] != attempt_id
            {
                return Err(Error::Invalid("步骤处理记录不匹配"));
            }
            attempt["status"] = json!("waiting_user");
            Self::save_expert_attempt_in(&mut transaction, account, &mut attempt).await?;
            task["steps"][index]["status"] = json!("waiting_user");
        }
        task["status"] = json!("waiting_user");
        task["current_step_id"] = json!(step_id);
        Self::save_expert_task_in(&mut transaction, account, run, &mut task).await?;
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        let card = json!({ "id": id, "session_id": run["session_id"], "run_id": run["id"], "expert_task_id": task["id"],
            "step_id": step_id, "attempt_id": attempt_id, "expert_card": content, "expected_task_revision": task["revision"],
            "status": "pending", "created_at": now, "updated_at": now });
        sqlx::query("INSERT INTO expert_cards(id,account_id,session_id,created_at,document,tool_call_id) VALUES(?,?,?,?,?,?)")
            .bind(&id).bind(account).bind(text(run, "session_id")?).bind(now).bind(card.to_string()).bind(call_id).execute(&mut *transaction).await?;
        Self::append_event_in(
            &mut transaction,
            account,
            text(run, "session_id")?,
            text(run, "id")?,
            json!({ "type": "CUSTOM", "name": "EXPERT_CARD_REQUIRED", "value": card }),
        )
        .await?;
        transaction.commit().await?;
        Ok(
            json!({ "status": "waiting_user", "card_id": id, "expert_task_id": task["id"], "step_id": step_id, "task_revision": task["revision"], "expert_card": content }),
        )
    }

    pub async fn resolve_expert_card_response(
        &self,
        account: &str,
        id: &str,
        input: Value,
    ) -> Result<Value> {
        let response: ExpertCardResponse =
            serde_json::from_value(input).map_err(|_| Error::Invalid("专家卡片响应格式无效"))?;
        if response.response_id.trim().is_empty() || response.response_id.len() > 256 {
            return Err(Error::Invalid("卡片响应标识无效"));
        }
        let card = self.expert_card(account, id).await?;
        if card["status"] != "pending" {
            if card["response"] != serde_json::to_value(&response)? {
                return Err(Error::Invalid("专家卡片已经收到其他答复"));
            }
            return self.get_run(account, text(&card, "response_run_id")?).await;
        }
        let mode = text(&card["expert_card"], "mode")?;
        let label = match (mode, response.action.as_str()) {
            ("step_entry", "start") => "开始步骤",
            ("step_entry", "defer") => "继续补充资料",
            ("step_entry", "skip") => "跳过步骤",
            ("result_review", "accept") => "确认结果",
            ("result_review", "revise") => "修改结果",
            _ => return Err(Error::Invalid("当前卡片不支持此项操作")),
        };
        let inputs = Value::Object(response.inputs.clone());
        validate_inputs(&card["expert_card"], &inputs, response.action == "start")?;
        let session_id = text(&card, "session_id")?;
        let expert = self
            .resolve_expert(account, session_id, &Value::Null)
            .await?
            .ok_or(Error::Invalid("当前会话没有绑定专家"))?;
        let content: crate::experts::ExpertContent = serde_json::from_value(expert.clone())?;
        let step = content
            .sop
            .iter()
            .find(|step| card["step_id"] == step.id)
            .ok_or(Error::Invalid("专家步骤不存在"))?;
        let mut message = format!("{label}「{}」", step.title);
        for field in card["expert_card"]
            .get("fields")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let key = text(field, "key")?;
            if ["asset", "assets"].contains(&text(field, "type")?)
                || inputs[key].is_null()
                || inputs[key] == ""
            {
                continue;
            }
            let value = match inputs[key].as_str() {
                Some(value) => value.to_owned(),
                None => serde_json::to_string(&inputs[key])?,
            };
            message.push_str(&format!("\n{}：{value}", text(field, "label")?));
        }
        if !response.comment.is_empty() {
            message.push_str(&format!("\n{}", response.comment));
        }
        let request_id = format!("expert-card:{}", response.response_id);
        let session = self.get_session(account, session_id).await?;
        let input = json!({ "session_id": session_id, "request_id": request_id, "text": message, "permission_mode": session.permission_mode,
            "expert_id": expert["id"], "expert_version": expert["version"], "model_config_id": Self::expert_model_id(&expert)?,
            "assets": input_assets(&card["expert_card"], &inputs)?, "expert_card_response": { "card_id": id, "response": response },
        });
        let decision = ExpertCardDecision {
            card_id: id.to_owned(),
            task_id: text(&card, "expert_task_id")?.to_owned(),
            response,
        };
        let queued = self
            .queue_run_input(
                account,
                session_id,
                &request_id,
                Self::expert_model_id(&expert)?,
                &message,
                &session.permission_mode,
                &input,
                Some(&decision),
            )
            .await?;
        Ok(queued["run"].clone())
    }

    pub(crate) async fn apply_expert_card_response_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        run: &Value,
        decision: &ExpertCardDecision,
    ) -> Result<()> {
        let saved: String =
            sqlx::query_scalar("SELECT document FROM expert_cards WHERE account_id=? AND id=?")
                .bind(account)
                .bind(&decision.card_id)
                .fetch_optional(&mut **transaction)
                .await?
                .ok_or(Error::NotFound)?;
        let mut card: Value = serde_json::from_str(&saved)?;
        if card["status"] != "pending"
            || card["session_id"] != run["session_id"]
            || card["expert_task_id"] != decision.task_id
        {
            return Err(Error::Invalid("专家卡片状态已更新"));
        }
        let mut task = Self::expert_task_in(transaction, account, &decision.task_id).await?;
        if task["revision"] != card["expected_task_revision"]
            || task["session_id"] != run["session_id"]
            || ["completed", "cancelled", "failed"].contains(&text(&task, "status")?)
        {
            return Err(Error::Invalid("专家任务已更新，请重新确认"));
        }
        let current: Option<String> = sqlx::query_scalar(
            "SELECT current_expert_task_id FROM sessions WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(text(run, "session_id")?)
        .fetch_one(&mut **transaction)
        .await?;
        if current.as_deref() != Some(decision.task_id.as_str()) {
            return Err(Error::Invalid("当前专家任务已更换"));
        }
        let step_id = text(&card, "step_id")?.to_owned();
        let index = task["steps"]
            .as_array()
            .ok_or(Error::State("专家步骤格式无效"))?
            .iter()
            .position(|step| step["step_id"] == step_id)
            .ok_or(Error::Invalid("专家步骤不存在"))?;
        let response = &decision.response;
        let inputs = Value::Object(response.inputs.clone());
        validate_inputs(&card["expert_card"], &inputs, response.action == "start")?;
        Self::validate_expert_result_in(
            transaction,
            account,
            text(run, "session_id")?,
            &json!({ "assets": input_assets(&card["expert_card"], &inputs)? }),
            false,
        )
        .await?;
        task["status"] = json!("active");
        match response.action.as_str() {
            "start" if card["expert_card"]["mode"] == "step_entry" => {
                let attempt = Self::start_expert_attempt_in(
                    transaction,
                    account,
                    run,
                    &mut task,
                    &step_id,
                    &inputs,
                )
                .await?;
                card["attempt_id"] = attempt["id"].clone();
            }
            "defer" if card["expert_card"]["mode"] == "step_entry" => {}
            "skip" if card["expert_card"]["mode"] == "step_entry" => {
                task["steps"][index]["status"] = json!("skipped");
                task["steps"][index]["summary"] = json!(response.comment);
            }
            "accept" | "revise" if card["expert_card"]["mode"] == "result_review" => {
                let attempt_id = text(&card, "attempt_id")?;
                if task["steps"][index]["latest_attempt_id"] != attempt_id {
                    return Err(Error::Invalid("结果卡片对应的处理记录已更新"));
                }
                let mut attempt = Self::expert_attempt_in(transaction, account, attempt_id).await?;
                if attempt["expert_task_id"] != task["id"] || attempt["step_id"] != step_id {
                    return Err(Error::Invalid("步骤处理记录不匹配"));
                }
                attempt["status"] = json!(if response.action == "accept" {
                    "completed"
                } else {
                    "running"
                });
                attempt["completed_at"] = if response.action == "accept" {
                    json!(timestamp()?)
                } else {
                    Value::Null
                };
                if response.action == "accept" {
                    Self::validate_expert_attempt_workflows_in(transaction, account, attempt_id)
                        .await?;
                    Self::validate_expert_result_in(
                        transaction,
                        account,
                        text(run, "session_id")?,
                        &card["expert_card"]["results"],
                        true,
                    )
                    .await?;
                    attempt["result"] = card["expert_card"]["results"].clone();
                }
                Self::save_expert_attempt_in(transaction, account, &mut attempt).await?;
                task["steps"][index]["status"] = attempt["status"].clone();
                task["steps"][index]["summary"] = card["expert_card"]["results"]["summary"]
                    .as_str()
                    .map(|summary| json!(summary))
                    .unwrap_or_else(|| json!(""));
            }
            _ => return Err(Error::Invalid("当前卡片不支持此项操作")),
        }
        Self::save_expert_task_in(transaction, account, run, &mut task).await?;
        card["status"] = json!("answered");
        card["response_action"] = json!(response.action);
        card["response"] = serde_json::to_value(response)?;
        card["response_json"] = json!({ "inputs": response.inputs, "comment": response.comment });
        card["response_run_id"] = run["id"].clone();
        card["response_message_id"] = run["trigger_message_id"].clone();
        card["updated_at"] = json!(timestamp()?);
        let changed = sqlx::query("UPDATE expert_cards SET document=? WHERE account_id=? AND id=? AND json_extract(document,'$.status')='pending'")
            .bind(card.to_string()).bind(account).bind(&decision.card_id).execute(&mut **transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Invalid("专家卡片已经收到答复"));
        }
        Ok(())
    }

    pub(crate) async fn validate_expert_start_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        run: &Value,
        step_id: &str,
    ) -> Result<()> {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM messages WHERE account_id=? AND session_id=? AND id=? AND json_extract(document,'$.role')='user')")
            .bind(account).bind(text(run, "session_id")?).bind(text(run, "trigger_message_id")?).fetch_one(&mut **transaction).await?;
        if !exists {
            return Err(Error::Invalid("步骤授权消息无效"));
        }
        if run["trigger_kind"] == "card_response" {
            let saved: String =
                sqlx::query_scalar("SELECT document FROM expert_cards WHERE account_id=? AND id=?")
                    .bind(account)
                    .bind(text(run, "trigger_record_id")?)
                    .fetch_one(&mut **transaction)
                    .await?;
            let card: Value = serde_json::from_str(&saved)?;
            if card["step_id"] != step_id
                || (card["response_action"] != "start" && card["response_action"] != "revise")
            {
                return Err(Error::Invalid("当前卡片决定没有授权开始此步骤"));
            }
        }
        Ok(())
    }
}
