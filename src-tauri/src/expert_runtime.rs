use crate::{
    Error, Result, Store,
    experts::{ExpertContent, ExpertPart},
    local_api::text,
    tools::{RunTool, ToolTarget},
};
use serde_json::{Value, json};

fn instruction_parts(parts: &[ExpertPart]) -> Result<String> {
    parts
        .iter()
        .map(|part| match part {
            ExpertPart::Text { text } => Ok(text.clone()),
            _ => Ok(serde_json::to_string(part)?),
        })
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("\n"))
}

impl Store {
    pub(crate) async fn expert_runtime_instruction(
        &self,
        run: &Value,
    ) -> Result<Option<String>> {
        if run["expert_id"].as_str().is_none_or(str::is_empty) {
            return Ok(None);
        }
        let session = self.get_session(text(run, "session_id")?).await?;
        let snapshot: String =
            sqlx::query_scalar("SELECT expert_snapshot FROM sessions WHERE id=?")
                .bind(&session.id)
                .fetch_one(&self.pool)
                .await?;
        let content: ExpertContent = serde_json::from_str(&snapshot)?;
        let mut instructions = format!(
            "\n专家提示词：\n{}\n\n完整 SOP：\n",
            instruction_parts(&content.prompt_parts)?
        );
        for step in &content.sop {
            instructions.push_str(&format!(
                "\n步骤 ID：{}\n名称：{}\n{}\n",
                step.id,
                step.title,
                instruction_parts(&step.instruction_parts)?
            ));
        }
        let mut task = Value::Null;
        let mut attempts = Vec::new();
        if session.current_expert_task_id.is_some() {
            task = self.current_expert_task(run).await?;
            let documents: Vec<String> = sqlx::query_scalar("SELECT document FROM expert_step_attempts WHERE expert_task_id=? ORDER BY created_at,id")
                .bind(text(&task, "id")?).fetch_all(&self.pool).await?;
            attempts = documents
                .iter()
                .map(|document| serde_json::from_str::<Value>(document))
                .collect::<std::result::Result<Vec<_>, _>>()?;
        }
        let decision = if run["trigger_kind"] == "card_response" {
            let document: String =
                sqlx::query_scalar("SELECT document FROM expert_cards WHERE id=?")
                    .bind(text(run, "trigger_record_id")?)
                    .fetch_one(&self.pool)
                    .await?;
            serde_json::from_str::<Value>(&document)?
        } else {
            Value::Null
        };
        let behavior = format!(
            "\n\n当前会话绑定固定专家版本。按照专家说明和完整 SOP 处理用户当前要求，由你选择进入、继续、返回或跳过步骤。使用 update_expert_task 保存实际任务与处理进度。进入步骤前通过 request_expert_confirmation 展示 step_entry 卡片；当前用户已经明确要求开始时，可以用 start_step 和真实 authorization_message_id 保存授权。卡片 start 和 revise 已经保存处理记录与输入，可以直接继续。defer 仅讨论和补充资料，skip 跳过对应步骤。写入作品需要正在执行的步骤处理记录。使用工具返回的真实消息、资产版本和工作流编号保存结果。结果需要确认时使用 result_review 卡片，用户 accept 后再完成任务。创建确认卡片后等待用户答复。用户没有要求执行步骤时可以直接回答问题。禁止编造用户决定、记录编号和工具结果。\n当前用户消息 ID：{}\n触发类型：{}\n当前任务和处理记录：{}\n当前卡片决定：{}\n",
            text(run, "trigger_message_id")?,
            text(run, "trigger_kind")?,
            serde_json::to_string(&json!({ "task": task, "attempts": attempts }))?,
            serde_json::to_string(&decision)?,
        );
        instructions.push_str(&behavior);
        Ok(Some(instructions))
    }

    pub(crate) async fn guard_expert_tool_write(
        &self,
        run: &Value,
        tool: &RunTool,
    ) -> Result<()> {
        if run["expert_id"].as_str().is_none_or(str::is_empty)
            || tool.read_only
            || matches!(tool.target, ToolTarget::Expert)
        {
            return Ok(());
        }
        if run["trigger_kind"] == "card_response" {
            let document: String =
                sqlx::query_scalar("SELECT document FROM expert_cards WHERE id=?")
                    .bind(text(run, "trigger_record_id")?)
                    .fetch_one(&self.pool)
                    .await?;
            let card: Value = serde_json::from_str(&document)?;
            if card["response_action"] == "defer" || card["response_action"] == "skip" {
                return Err(Error::Invalid("当前卡片决定没有授权执行此步骤"));
            }
        }
        let task = self.current_expert_task(run).await?;
        if task["status"] != "active" && task["status"] != "waiting_tool" {
            return Err(Error::Invalid("当前专家任务尚未开始执行"));
        }
        let step = task["steps"]
            .as_array()
            .ok_or(Error::State("专家步骤格式无效"))?
            .iter()
            .find(|step| step["step_id"] == task["current_step_id"])
            .ok_or(Error::Invalid("请先开始专家步骤"))?;
        if !matches!(step["status"].as_str(), Some("running" | "waiting_tool")) {
            return Err(Error::Invalid("当前专家步骤尚未开始执行"));
        }
        let mut transaction = self.pool.begin().await?;
        let attempt = Self::expert_attempt_in(
            &mut transaction,
            text(step, "latest_attempt_id").map_err(|_| Error::Invalid("请先开始专家步骤"))?,
        )
        .await?;
        if attempt["status"] != "running" && attempt["status"] != "waiting_tool" {
            return Err(Error::Invalid("当前步骤正在等待用户确认或已经结束"));
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn record_expert_asset(
        &self,
        run: &Value,
        asset: &Value,
    ) -> Result<()> {
        if run["expert_id"].as_str().is_none_or(str::is_empty) {
            return Ok(());
        }
        let task = self.current_expert_task(run).await?;
        let step = task["steps"]
            .as_array()
            .ok_or(Error::State("专家步骤格式无效"))?
            .iter()
            .find(|step| step["step_id"] == task["current_step_id"])
            .ok_or(Error::Invalid("当前专家步骤不存在"))?;
        let version = asset["versions"]
            .as_array()
            .ok_or(Error::State("资产版本格式无效"))?
            .iter()
            .find(|version| version["version"] == asset["current_version"])
            .ok_or(Error::State("资产当前版本不存在"))?;
        let reference = json!({ "asset_id": asset["id"], "asset_version_id": version["id"] });
        let mut transaction = self.pool.begin().await?;
        let mut attempt =
            Self::expert_attempt_in(&mut transaction, text(step, "latest_attempt_id")?)
                .await?;
        if attempt["result"]["assets"].is_null() {
            attempt["result"]["assets"] = json!([]);
        }
        let references = attempt["result"]["assets"]
            .as_array_mut()
            .ok_or(Error::State("步骤结果资产格式无效"))?;
        if !references.contains(&reference) {
            references.push(reference);
        }
        Self::save_expert_attempt_in(&mut transaction, &mut attempt).await?;
        transaction.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn skipped_expert_steps_cannot_start_write_tools()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("expert-write-permission-")
            .tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let model = store.save_model(None, json!({
            "name": "本地配置", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:11434/v1", "model": "local-model", "api_key": "",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true },
        })).await?;
        let expert = store.save_expert(None, json!({
            "schema_version": 2, "name": "步骤权限", "description": "检查真实步骤决定", "model_id": model["id"],
            "prompt_parts": [{ "type": "text", "text": "按照用户决定执行步骤。" }],
            "avatar_asset_id": "", "avatar_version_id": "", "examples": [], "skill_refs": [], "workflow_refs": [],
            "sop": [{ "id": "write", "title": "保存作品", "instruction_parts": [{ "type": "text", "text": "保存用户作品。" }] }],
            "version": "1.0.0", "enabled": true,
        })).await?;
        let turn = store.submit_message(json!({
            "request_id": "step-decisions", "text": "开始检查步骤权限，然后跳过该步骤。", "permission_mode": "full_access", "expert_id": expert["id"],
        })).await?;
        let run_id = text(&turn["run"], "id")?;
        let run = store.get_run(run_id).await?;
        let instruction = store
            .expert_runtime_instruction(&run)
            .await?
            .ok_or("专家系统说明缺失")?;
        assert!(instruction.contains("按照用户决定执行步骤。"));
        assert!(instruction.contains("保存用户作品。"));
        let control = rig::message::ToolName::new("update_expert_task")?;
        store
            .call_expert_control(
                run_id,
                "create",
                &control,
                &json!({ "action": "create_task", "goal": "检查步骤权限" }),
            )
            .await?;
        let tool = store
            .run_tools(run_id)
            .await?
            .into_iter()
            .find(|tool| tool.display_name() == "create_text_asset")
            .ok_or("资产写入工具缺失")?;
        assert!(
            store
                .guard_expert_tool_write(&run, &tool)
                .await
                .is_err()
        );
        store.call_expert_control(run_id, "start", &control, &json!({
            "action": "start_step", "step_id": "write", "authorization_message_id": run["trigger_message_id"], "inputs": {},
        })).await?;
        store.guard_expert_tool_write(&run, &tool).await?;
        store.call_expert_control(run_id, "skip", &control, &json!({
            "action": "skip_step", "step_id": "write", "authorization_message_id": run["trigger_message_id"], "check_summary": "用户已经要求跳过。",
        })).await?;
        assert!(
            store
                .guard_expert_tool_write(&run, &tool)
                .await
                .is_err()
        );
        store.close().await;
        Ok(())
    }
}
