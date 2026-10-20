use crate::{
    Error, Result, Store,
    assets::digest,
    error::{required, timestamp},
    local_api::text,
    settings::ModelInput,
};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub(crate) fn checkpoint_version() -> String {
    format!("rig:0.44.0/{}", rig::run::RUN_FORMAT)
}

impl Store {
    pub async fn submit_message(&self, account: &str, mut input: Value) -> Result<Value> {
        Self::check_account(account)?;
        let fields = input
            .as_object_mut()
            .ok_or(Error::Invalid("消息内容格式无效"))?;
        if fields.get("request_id").is_none_or(Value::is_null) {
            fields.insert("request_id".to_owned(), json!(Uuid::new_v4().to_string()));
        }
        let request = text(&input, "request_id")?;
        required(request, "请求标识不能为空")?;
        let previous =
            sqlx::query("SELECT id,document FROM runs WHERE account_id=? AND request_id=?")
                .bind(account)
                .bind(request)
                .fetch_optional(&self.pool)
                .await?;
        if let Some(previous) = previous {
            let document: Value = serde_json::from_str(previous.get("document"))?;
            if document["input"] != input {
                return Err(Error::Invalid("请求标识已用于其他消息"));
            }
            return self.run_turn(account, previous.get("id")).await;
        }
        let content = text(&input, "text")?;
        required(content, "消息内容不能为空")?;
        let permission = text(&input, "permission_mode")?;
        if !["request_approval", "auto_approve", "full_access"].contains(&permission) {
            return Err(Error::Invalid("权限模式无效"));
        }
        let session = input["session_id"].as_str().unwrap_or("");
        let expert = self.resolve_expert(account, session, &input).await?;
        let requested_model = input["model_config_id"]
            .as_str()
            .filter(|id| !id.is_empty());
        let model = match (&expert, requested_model) {
            (Some(expert), requested) => {
                let model = Self::expert_model_id(expert)?;
                if requested.is_some_and(|requested| requested != model) {
                    return Err(Error::Invalid("专家会话不能更换绑定的本地模型"));
                }
                model.to_owned()
            }
            (None, Some(model)) => model.to_owned(),
            (None, None) => sqlx::query_scalar("SELECT id FROM model_configs WHERE account_id=? AND json_extract(document,'$.default')=1 AND json_extract(document,'$.enabled')=1 AND json_extract(document,'$.agent_enabled')=1")
                .bind(account).fetch_optional(&self.pool).await?.ok_or(Error::Invalid("请选择可用模型"))?,
        };
        self.queue_run_input(
            account, session, request, &model, content, permission, &input, None,
        )
        .await
    }

    pub async fn queue_run(
        &self,
        account: &str,
        session: &str,
        request: &str,
        model: &str,
        content: &str,
        permission: &str,
    ) -> Result<Value> {
        self.queue_run_input(
            account,
            session,
            request,
            model,
            content,
            permission,
            &Value::Null,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn queue_run_input(
        &self,
        account: &str,
        session: &str,
        request: &str,
        model: &str,
        content: &str,
        permission: &str,
        input: &Value,
        card: Option<&crate::expert_cards::ExpertCardDecision>,
    ) -> Result<Value> {
        Self::check_account(account)?;
        let _model_update = self.model_updates.lock().await;
        let create_session = session.is_empty() && input.is_object();
        if !create_session {
            self.get_session(account, session).await?;
        }
        required(request, "请求标识不能为空")?;
        required(content, "消息内容不能为空")?;
        if !["request_approval", "auto_approve", "full_access"].contains(&permission) {
            return Err(Error::Invalid("权限模式无效"));
        }
        let fingerprint = digest(&serde_json::to_vec(&(
            session, model, content, permission, input,
        ))?);
        let previous = sqlx::query(
            "SELECT id,request_digest,document FROM runs WHERE account_id=? AND request_id=?",
        )
        .bind(account)
        .bind(request)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(previous) = previous {
            let same_request = if input.is_object() {
                serde_json::from_str::<Value>(previous.get("document"))?["input"] == *input
            } else {
                previous.get::<String, _>("request_digest") == fingerprint
            };
            if !same_request {
                return Err(Error::Invalid("请求标识已用于其他消息"));
            }
            return self
                .run_turn(account, &previous.get::<String, _>("id"))
                .await;
        }
        let expert = self.resolve_expert(account, session, input).await?;
        if let Some(expert) = &expert
            && Self::expert_model_id(expert)? != model
        {
            return Err(Error::Invalid("专家会话不能更换绑定的本地模型"));
        }
        let model_row = sqlx::query(
            "SELECT document,credential_ref FROM model_configs WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(model)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)?;
        let current: ModelInput = serde_json::from_str(model_row.get("document"))?;
        if !current.enabled || !current.agent_enabled {
            return Err(Error::Invalid("模型尚未启用 Agent 调用"));
        }
        let snapshot = if let Some(expert) = expert
            .as_ref()
            .filter(|expert| expert["source"] == "official")
        {
            expert
                .get("runtime_model_snapshot")
                .filter(|snapshot| snapshot.is_object())
                .cloned()
                .ok_or(Error::State("专家会话没有保存完整模型配置"))?
        } else {
            json!({ "configuration": current, "credential_ref": model_row.get::<Option<String>, _>("credential_ref") })
        };
        let mut configuration: ModelInput =
            serde_json::from_value(snapshot["configuration"].clone())?;
        configuration.validate()?;
        if !configuration.enabled || !configuration.agent_enabled {
            return Err(Error::Invalid("模型尚未启用 Agent 调用"));
        }
        if let Some(expert) = &expert {
            let mut bound = expert.clone();
            bound["enabled"] = json!(true);
            if let Some(reason) = self.expert_unavailable_reason(account, &bound).await? {
                return Err(Error::Invalid(reason));
            }
        }
        let context_input = Self::expert_context_input(input, expert.as_ref())?;
        let (context, prompt) = self
            .prepare_run_context(
                account,
                &context_input,
                &configuration,
                content,
                expert.as_ref(),
            )
            .await?;
        let id = Uuid::new_v4().to_string();
        let message_id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        let project = input["project_id"].as_str().filter(|id| !id.is_empty());
        if create_session && let Some(project) = project {
            self.get_project(account, project).await?;
        }
        let mut transaction = self.pool.begin().await?;
        let session_id = if create_session {
            sqlx::query("INSERT INTO sessions(id,account_id,request_id,title,project_id,created_at,updated_at) VALUES(?,?,?,'新对话',?,?,?) ON CONFLICT(account_id,request_id) DO NOTHING")
                .bind(Uuid::new_v4().to_string()).bind(account).bind(request).bind(project).bind(&now).bind(&now).execute(&mut *transaction).await?;
            let saved = sqlx::query(
                "SELECT id,project_id FROM sessions WHERE account_id=? AND request_id=?",
            )
            .bind(account)
            .bind(request)
            .fetch_one(&mut *transaction)
            .await?;
            if saved.get::<Option<String>, _>("project_id").as_deref() != project {
                return Err(Error::Invalid("请求标识已用于其他项目"));
            }
            saved.get::<String, _>("id")
        } else {
            session.to_owned()
        };
        let session = session_id.as_str();
        let fingerprint = digest(&serde_json::to_vec(&(
            session, model, content, permission, input,
        ))?);
        let checkpoint = rig::AgentRun::new(prompt).max_turns(32);
        let message = json!({ "id": message_id, "session_id": session, "run_id": id, "role": "user", "content": [{ "type": "text", "text": content }], "parts": Self::context_asset_parts(&context, input), "created_at": now });
        let run = json!({ "id": id, "session_id": session, "trigger_message_id": message_id, "trigger_kind": if card.is_some() { "card_response" } else { "user_message" },
            "trigger_record_id": card.map(|card| card.card_id.as_str()).unwrap_or(""), "expert_task_id": card.map(|card| card.task_id.as_str()),
            "status": "queued", "model_config_id": model, "permission_mode": permission, "input": input,
            "expert_id": expert.as_ref().map(|expert| expert["id"].clone()), "expert_version": expert.as_ref().map(|expert| expert["version"].clone()),
            "created_at": now, "updated_at": now });
        let inserted: Option<String> = sqlx::query_scalar("INSERT INTO runs(id,account_id,session_id,created_at,request_id,document,checkpoint,checkpoint_version,request_digest,model_snapshot,context_snapshot) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(account_id,request_id) DO NOTHING RETURNING id")
            .bind(&id).bind(account).bind(session).bind(&now).bind(request).bind(run.to_string()).bind(serde_json::to_string(&checkpoint)?).bind(checkpoint_version()).bind(&fingerprint).bind(snapshot.to_string()).bind(serde_json::to_string(&context)?).fetch_optional(&mut *transaction).await?;
        if inserted.is_none() {
            let previous = sqlx::query(
                "SELECT id,request_digest FROM runs WHERE account_id=? AND request_id=?",
            )
            .bind(account)
            .bind(request)
            .fetch_one(&mut *transaction)
            .await?;
            if previous.get::<String, _>("request_digest") != fingerprint {
                return Err(Error::Invalid("请求标识已用于其他消息"));
            }
            let previous: String = previous.get("id");
            transaction.commit().await?;
            return self.run_turn(account, &previous).await;
        }
        sqlx::query(
            "INSERT INTO messages(id,account_id,session_id,created_at,document) VALUES(?,?,?,?,?)",
        )
        .bind(&message_id)
        .bind(account)
        .bind(session)
        .bind(&now)
        .bind(message.to_string())
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE sessions SET permission_mode=?,model_config_id=?,updated_at=? WHERE account_id=? AND id=?")
            .bind(permission).bind(model).bind(&now).bind(account).bind(session).execute(&mut *transaction).await?;
        if let Some(expert) = &expert {
            sqlx::query("UPDATE sessions SET expert_id=?,expert_version=?,expert_name=?,expert_snapshot=coalesce(expert_snapshot,?) WHERE account_id=? AND id=?")
                .bind(text(expert, "id")?).bind(text(expert, "version")?).bind(text(expert, "name")?).bind(expert.to_string())
                .bind(account).bind(session).execute(&mut *transaction).await?;
        }
        if let Some(card) = card {
            Self::apply_expert_card_response_in(&mut transaction, account, &run, card).await?;
        }
        Self::reference_run_assets(&mut transaction, account, &id, session, &context, &now).await?;
        transaction.commit().await?;
        self.run_turn(account, &id).await
    }

    pub async fn get_run(&self, account: &str, id: &str) -> Result<Value> {
        Self::check_account(account)?;
        let document: String =
            sqlx::query_scalar("SELECT document FROM runs WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Error::NotFound)?;
        Ok(serde_json::from_str(&document)?)
    }

    async fn run_turn(&self, account: &str, id: &str) -> Result<Value> {
        let run = self.get_run(account, id).await?;
        let session = self.get_session(account, text(&run, "session_id")?).await?;
        let message: String =
            sqlx::query_scalar("SELECT document FROM messages WHERE account_id=? AND id=?")
                .bind(account)
                .bind(text(&run, "trigger_message_id")?)
                .fetch_one(&self.pool)
                .await?;
        Ok(
            json!({ "session": session, "message": serde_json::from_str::<Value>(&message)?, "run": run }),
        )
    }

    pub async fn run_checkpoint(&self, account: &str, id: &str) -> Result<rig::AgentRun> {
        Self::check_account(account)?;
        let row = sqlx::query(
            "SELECT checkpoint,checkpoint_version FROM runs WHERE account_id=? AND id=?",
        )
        .bind(account)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)?;
        if row
            .get::<Option<String>, _>("checkpoint_version")
            .as_deref()
            != Some(checkpoint_version().as_str())
        {
            return Err(Error::State("运行检查点与当前 Rig 版本不兼容"));
        }
        Ok(serde_json::from_str(
            &row.get::<Option<String>, _>("checkpoint")
                .ok_or(Error::State("运行检查点不存在"))?,
        )?)
    }

    pub async fn append_run_event(&self, account: &str, id: &str, event: Value) -> Result<Value> {
        let run = self.get_run(account, id).await?;
        let mut transaction = self.pool.begin().await?;
        let document = Self::append_event_in(
            &mut transaction,
            account,
            text(&run, "session_id")?,
            id,
            event,
        )
        .await?;
        transaction.commit().await?;
        Ok(document)
    }

    pub(crate) async fn append_event_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        session: &str,
        id: &str,
        event: Value,
    ) -> Result<Value> {
        required(text(&event, "type")?, "事件类型不能为空")?;
        let event_id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        let document = json!({ "id": event_id, "run_id": id, "type": event["type"], "payload": event, "created_at": now });
        let document: String = sqlx::query_scalar("INSERT INTO run_events(id,account_id,session_id,run_id,sequence,created_at,document) SELECT ?,?,?,?,coalesce(max(sequence),0)+1,?,json_set(json(?),'$.sequence',coalesce(max(sequence),0)+1,'$.payload.sequence',coalesce(max(sequence),0)+1) FROM run_events WHERE account_id=? AND run_id=? RETURNING document")
            .bind(&event_id).bind(account).bind(session).bind(id).bind(&now).bind(document.to_string()).bind(account).bind(id).fetch_one(&mut **transaction).await?;
        Ok(serde_json::from_str(&document)?)
    }

    pub async fn run_events_after(
        &self,
        account: &str,
        id: &str,
        after: i64,
    ) -> Result<Vec<Value>> {
        self.get_run(account, id).await?;
        if after < 0 {
            return Err(Error::Invalid("事件序号不能小于零"));
        }
        let documents: Vec<String> = sqlx::query_scalar("SELECT document FROM run_events WHERE account_id=? AND run_id=? AND sequence>? ORDER BY sequence")
            .bind(account).bind(id).bind(after).fetch_all(&self.pool).await?;
        documents
            .iter()
            .map(|document| serde_json::from_str(document).map_err(Error::from))
            .collect()
    }

    pub async fn claim_run(&self, account: &str, id: &str) -> Result<bool> {
        self.get_run(account, id).await?;
        let now = timestamp()?;
        let changed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','running','$.started_at',?,'$.updated_at',?) WHERE account_id=? AND id=? AND status='queued' AND NOT EXISTS(SELECT 1 FROM runs active WHERE active.account_id=runs.account_id AND active.session_id=runs.session_id AND active.status IN ('running','waiting_approval','waiting_clarification','waiting_workflow')) AND NOT EXISTS(SELECT 1 FROM runs earlier WHERE earlier.account_id=runs.account_id AND earlier.session_id=runs.session_id AND earlier.status='queued' AND (earlier.created_at<runs.created_at OR (earlier.created_at=runs.created_at AND earlier.id<runs.id)))")
            .bind(&now).bind(&now).bind(account).bind(id).execute(&self.pool).await?;
        Ok(changed.rows_affected() == 1)
    }

    pub async fn recover_interrupted_runs(&self) -> Result<u64> {
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        let resumed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','queued','$.updated_at',?) WHERE status IN ('running','waiting_workflow') AND EXISTS(SELECT 1 FROM workflow_executions w JOIN tool_calls t ON t.account_id=w.account_id AND t.run_id=w.run_id AND t.id=w.id WHERE w.account_id=runs.account_id AND w.run_id=runs.id AND coalesce(json_extract(w.document,'$.task_id'),'')<>'' AND t.status='waiting_workflow') AND NOT EXISTS(SELECT 1 FROM tool_calls t WHERE t.run_id=runs.id AND t.status='executing')")
            .bind(&now).execute(&mut *transaction).await?.rows_affected();
        let mcp_resumed = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','queued','$.updated_at',?) WHERE status='running' AND EXISTS(SELECT 1 FROM mcp_executions m JOIN tool_calls t ON t.account_id=m.account_id AND t.run_id=m.run_id AND t.id=m.id WHERE m.account_id=runs.account_id AND m.run_id=runs.id AND (json_extract(m.document,'$.status') IN ('input_required','ready','completed','failed','cancelled') OR (json_extract(m.document,'$.status')='polling' AND coalesce(json_extract(m.document,'$.task.taskId'),'')<>'')) AND t.status='waiting_mcp') AND NOT EXISTS(SELECT 1 FROM tool_calls t WHERE t.run_id=runs.id AND t.status='executing')")
            .bind(&now).execute(&mut *transaction).await?.rows_affected();
        sqlx::query("UPDATE runs SET document=json_set(document,'$.status','running') WHERE status='waiting_clarification' AND EXISTS(SELECT 1 FROM mcp_executions m JOIN tool_calls t ON t.account_id=m.account_id AND t.run_id=m.run_id AND t.id=m.id WHERE m.account_id=runs.account_id AND m.run_id=runs.id AND json_extract(m.document,'$.input.source')='live' AND t.status='executing')")
            .execute(&mut *transaction).await?;
        let rows = sqlx::query("UPDATE runs SET document=json_set(document,'$.status','failed','$.error_code',CASE WHEN EXISTS(SELECT 1 FROM tool_calls t WHERE t.run_id=runs.id AND t.status='executing') THEN 'tool_result_unknown' ELSE 'runtime_interrupted' END,'$.error_message',CASE WHEN EXISTS(SELECT 1 FROM tool_calls t WHERE t.run_id=runs.id AND t.status='executing') THEN '工具执行结果未知，禁止自动重复执行' ELSE 'App 已重新启动，运行已中断' END,'$.completed_at',?,'$.updated_at',?) WHERE status='running' RETURNING id,account_id,session_id,document")
            .bind(&now).bind(&now).fetch_all(&mut *transaction).await?;
        for row in &rows {
            let run: Value = serde_json::from_str(row.get("document"))?;
            Self::append_event_in(&mut transaction, row.get("account_id"), row.get("session_id"), row.get("id"), json!({ "type": "RUN_ERROR", "code": run["error_code"], "message": run["error_message"] })).await?;
            sqlx::query("UPDATE clarifications SET document=json_set(document,'$.status','cancelled','$.updated_at',?) WHERE account_id=? AND run_id=? AND json_extract(document,'$.status')='pending'")
                .bind(&now).bind(row.get::<String, _>("account_id")).bind(row.get::<String, _>("id")).execute(&mut *transaction).await?;
            sqlx::query("UPDATE messages SET document=json_set(document,'$.clarification.status','skipped') WHERE account_id=? AND json_extract(document,'$.run_id')=? AND json_extract(document,'$.clarification.status')='pending'")
                .bind(row.get::<String, _>("account_id")).bind(row.get::<String, _>("id")).execute(&mut *transaction).await?;
        }
        sqlx::query("UPDATE tool_calls SET status='result_unknown',updated_at=?,ended_at=? WHERE status='executing'")
            .bind(&now).bind(&now).execute(&mut *transaction).await?;
        sqlx::query("UPDATE workflow_executions SET document=json_set(document,'$.status','failed','$.submission_state','result_unknown','$.error_message','工作流提交没有取得任务编号，禁止自动重复提交','$.completed_at',?) WHERE json_extract(document,'$.status')='submitted' AND coalesce(json_extract(document,'$.task_id'),'')=''")
            .bind(&now).execute(&mut *transaction).await?;
        sqlx::query("UPDATE model_requests SET document=json_set(document,'$.status','interrupted','$.ended_at',?,'$.error_message','App 已重新启动，模型请求已中断') WHERE json_extract(document,'$.status')='running'")
            .bind(&now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(rows.len() as u64 + resumed + mcp_resumed)
    }
}
