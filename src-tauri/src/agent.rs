use crate::{
    Error, Result, Store,
    models::{build_model, completion_request},
};
use futures_util::StreamExt;
use rig::{
    completion::{CompletionRequest, FinishReason},
    message::{AssistantContent, UserContent},
    run::{AgentRunStep, ModelTurn, ModelTurnOutcome, prepare::prepare_request, spec::RunSpec},
    streaming::{Item, PartKind, StreamEvent},
};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, RwLock, watch};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct AgentRuntime {
    pub(crate) store: Store,
    active: Arc<Mutex<HashMap<String, CancellationToken>>>,
    dispatch: Arc<RwLock<()>>,
    changes: watch::Sender<u64>,
}

impl AgentRuntime {
    pub fn new(store: Store) -> Self {
        let changes = store.run_changes.clone();
        Self {
            store,
            active: Arc::new(Mutex::new(HashMap::new())),
            dispatch: Arc::new(RwLock::new(())),
            changes,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    pub async fn resume_pending(&self) -> Result<()> {
        let sessions = self.store.queued_sessions().await?;
        let results = futures_util::future::join_all(
            sessions
                .iter()
                .map(|session| self.drain_session(session)),
        )
        .await;
        for result in results {
            result?;
        }
        Ok(())
    }

    pub async fn drain_session(&self, session: &str) -> Result<()> {
        let _dispatch = self.dispatch.read().await;
        while let Some(id) = self.store.next_queued_run(session).await? {
            let run = self.execute(&id).await?;
            if run["status"] == "queued" {
                return Ok(());
            }
        }
        Ok(())
    }

    pub async fn cancel_all(&self) -> Result<()> {
        let mut changes = self.subscribe();
        {
            let active = self.active.lock().await;
            for cancellation in active.values() {
                cancellation.cancel();
            }
        }
        for id in self.store.unfinished_runs().await? {
            self.cancel(&id).await?;
        }
        loop {
            changes.borrow_and_update();
            if self.store.unfinished_runs().await?.is_empty() {
                break;
            }
            changes
                .changed()
                .await
                .map_err(|_| Error::State("运行事件通知已关闭"))?;
        }
        let _dispatch = self.dispatch.write().await;
        Ok(())
    }

    pub async fn forward_events(
        &self,
        id: &str,
        mut after: i64,
        channel: tauri::ipc::Channel<Value>,
        cancellation: CancellationToken,
    ) -> Result<()> {
        let mut changes = self.subscribe();
        loop {
            changes.borrow_and_update();
            for event in self.store.run_events_after(id, after).await? {
                after = event["sequence"]
                    .as_i64()
                    .ok_or(Error::State("运行事件缺少序号"))?;
                let terminal = matches!(event["type"].as_str(), Some("RUN_FINISHED" | "RUN_ERROR"));
                channel
                    .send(event["payload"].clone())
                    .map_err(|_| Error::State("运行事件订阅已关闭"))?;
                if terminal {
                    return Ok(());
                }
            }
            let run = self.store.get_run(id).await?;
            if matches!(
                run["status"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                // 结束状态与结束事件在同一个事务中保存，需要再读取一次事件。
                for event in self.store.run_events_after(id, after).await? {
                    channel
                        .send(event["payload"].clone())
                        .map_err(|_| Error::State("运行事件订阅已关闭"))?;
                }
                return Ok(());
            }
            tokio::select! {
                _ = cancellation.cancelled() => return Ok(()),
                changed = changes.changed() => changed.map_err(|_| Error::State("运行事件通知已关闭"))?,
            }
        }
    }

    fn notify(&self) {
        self.store.notify_run_events();
    }

    async fn emit(&self, id: &str, event: Value) -> Result<()> {
        self.store.append_run_event(id, event).await?;
        self.notify();
        Ok(())
    }

    pub async fn execute(&self, id: &str) -> Result<Value> {
        let mut active = self.active.lock().await;
        if !self.store.claim_run(id).await? {
            return self.store.get_run(id).await;
        }
        let cancellation = CancellationToken::new();
        active.insert(id.to_owned(), cancellation.clone());
        drop(active);
        let outcome = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(Error::Cancelled),
            outcome = self.drive(id) => outcome,
        };
        self.active.lock().await.remove(id);
        let result = match outcome {
            Ok(()) => {
                let run = self.store.get_run(id).await?;
                if matches!(
                    run["status"].as_str(),
                    Some("waiting_approval" | "waiting_clarification" | "waiting_workflow")
                ) {
                    run
                } else {
                    self.store
                        .finish_run(id, "succeeded", None)
                        .await?
                }
            }
            Err(Error::Cancelled) => {
                self.store
                    .finish_run(id, "cancelled", None)
                    .await?
            }
            Err(error) => {
                self.store
                    .finish_run(id, "failed", Some(&error))
                    .await?
            }
        };
        self.notify();
        Ok(result)
    }

    pub async fn cancel(&self, id: &str) -> Result<()> {
        self.store.get_run(id).await?;
        let active = self.active.lock().await;
        if let Some(cancellation) = active.get(id) {
            cancellation.cancel();
        } else {
            self.store
                .finish_run(id, "cancelled", None)
                .await?;
            self.notify();
        }
        Ok(())
    }

    async fn drive(&self, id: &str) -> Result<()> {
        let run = self.store.get_run(id).await?;
        self.emit(id, json!({ "type": "RUN_STARTED", "threadId": run["session_id"], "runId": id, "metadata": { "studioRunId": id } })).await?;
        let configuration = self.store.run_model(id).await?;
        let model = build_model(&configuration)?;
        let mut checkpoint = self.store.run_checkpoint(id).await?;
        if checkpoint.initial_prompt().is_some() {
            checkpoint =
                checkpoint.with_history(self.store.previous_run_history(id).await?);
        }
        let mut spec = RunSpec::new();
        spec.preamble = Some("你是 Pixoma Studio 的创作助手。使用当前会话提供的资料完成用户要求，准确说明工具执行结果。视频创作使用当前会话的视频项目：通过 studio_read_video_project 读取最新项目，按用户指定的整片或分镜范围修改。单分镜修改优先使用 studio_update_video_shot，只提交需要修改的字段；inputs需提交完整生成参数。创建项目或调整完整分镜列表时使用 studio_write_video_project，完整保留未修改分镜。同id分镜省略媒体和生成状态字段时会保留现有值，只有用户要求清除采用视频时才设置media:null；省略项目audio会保留音轨。生成视频前读取所选工作流的固定版本与输入要求，使用 studio_run_workflow 执行；工具返回执行状态与结果后，使用 studio_apply_video_result 采用到对应分镜。生成素材的时长与时间轴剪辑时长分别设置。每次写入或采用前读取最新 asset_version；遇到版本冲突时重新读取并应用本次修改。需要导出时使用 studio_export_video。只有工具确认成功后才能说明内容已保存、视频已生成或成片已导出。".to_owned());
        if let Some(instruction) = self.store.expert_runtime_instruction(&run).await? {
            spec.preamble
                .as_mut()
                .ok_or(Error::State("系统提示词缺失"))?
                .push_str(&instruction);
        }
        spec.max_tokens = Some(configuration.limits.max_output_tokens);
        let request_settings = completion_request(&configuration, "")?;
        spec.options = request_settings.options;
        spec.additional_params = request_settings.additional_params;
        let tools = if configuration.capabilities.tools {
            self.store.run_tools(id).await?
        } else {
            Vec::new()
        };
        loop {
            match checkpoint.next_step()? {
                AgentRunStep::CallModel {
                    prompt,
                    history,
                    turn,
                } => {
                    let prepared = prepare_request(
                        &spec,
                        &model.capabilities().completion,
                        &history,
                        tools.iter().map(|tool| tool.definition.clone()).collect(),
                        None,
                        None,
                    )
                    .map_err(rig::error::ProviderError::from)?;
                    checkpoint.advertise_tools(turn, prepared.tools.clone());
                    self.store
                        .save_run_checkpoint(id, &checkpoint)
                        .await?;
                    let request = prepared.clone().apply(CompletionRequest::new(prompt));
                    let request = self
                        .store
                        .project_model_context(id, turn, &configuration, &model, request)
                        .await?;
                    self.notify();
                    let attempt = self
                        .store
                        .begin_model_request(id, turn, &configuration, &request, "agent")
                        .await?;
                    let response = if configuration.capabilities.streaming {
                        let mut stream = model.stream(request)?;
                        let mut received = false;
                        while let Some(item) = stream.next().await {
                            if let Item::Event(event) = item? {
                                if !received {
                                    self.store.mark_model_first_token(&attempt).await?;
                                    received = true;
                                }
                                self.stream_event(id, turn, event).await?;
                            }
                        }
                        stream.finish().await?
                    } else {
                        let response = model.call(request).await?;
                        for (index, content) in response.choice.iter().enumerate() {
                            let part = index;
                            let message = format!("{id}-{turn}-{part}");
                            match content {
                                AssistantContent::Text(text) if !text.text.is_empty() => {
                                    self.emit(id, json!({ "type": "TEXT_MESSAGE_START", "messageId": message, "role": "assistant" })).await?;
                                    self.emit(id, json!({ "type": "TEXT_MESSAGE_CONTENT", "messageId": message, "delta": text.text })).await?;
                                    self.emit(
                                        id,
                                        json!({ "type": "TEXT_MESSAGE_END", "messageId": message }),
                                    )
                                    .await?;
                                }
                                AssistantContent::Reasoning(reasoning)
                                    if !reasoning.text.is_empty() =>
                                {
                                    self.emit(id, json!({ "type": "REASONING_MESSAGE_START", "messageId": message })).await?;
                                    self.emit(id, json!({ "type": "REASONING_MESSAGE_CONTENT", "messageId": message, "delta": reasoning.text })).await?;
                                    self.emit(id, json!({ "type": "REASONING_MESSAGE_END", "messageId": message })).await?;
                                }
                                AssistantContent::ToolCall(call) => {
                                    self.store
                                        .save_tool_part(id, turn, part, call)
                                        .await?;
                                    self.emit(id, json!({ "type": "TOOL_CALL_START", "toolCallId": message, "toolCallName": self.store.tool_display_name(id, &call.function.name).await? })).await?;
                                    self.emit(id, json!({ "type": "TOOL_CALL_ARGS", "toolCallId": message, "delta": serde_json::to_string(&call.function.arguments)? })).await?;
                                    self.emit(
                                        id,
                                        json!({ "type": "TOOL_CALL_END", "toolCallId": message }),
                                    )
                                    .await?;
                                }
                                AssistantContent::Text(_) | AssistantContent::Reasoning(_) => {}
                                _ => return Err(Error::Invalid("调用的模型输出尚未注册")),
                            }
                        }
                        response
                    };
                    self.store
                        .finish_model_request(&attempt, &response)
                        .await?;
                    self.store
                        .save_assistant_turn(id, turn, &response)
                        .await?;
                    let outcome =
                        checkpoint.model_response(ModelTurn::from_response(&response, &prepared));
                    self.store
                        .save_run_checkpoint(id, &checkpoint)
                        .await?;
                    self.emit(
                        id,
                        json!({ "type": "CUSTOM", "name": "usage", "value": response.usage }),
                    )
                    .await?;
                    match response.finish_reason() {
                        Some(FinishReason::Length) => {
                            return Err(Error::Invalid("模型回复达到输出限制，已保留生成内容"));
                        }
                        Some(FinishReason::Other(reason))
                            if configuration.protocol == "openai_responses"
                                && reason == "incomplete: length" =>
                        {
                            return Err(Error::Invalid("模型回复达到输出限制，已保留生成内容"));
                        }
                        Some(FinishReason::ContentFilter) => {
                            return Err(Error::Invalid("模型服务已停止输出"));
                        }
                        _ => {}
                    }
                    if matches!(outcome?, ModelTurnOutcome::NeedsResolution(_)) {
                        checkpoint.resolve_unhandled_invalid_tool_call()?;
                        self.store
                            .save_run_checkpoint(id, &checkpoint)
                            .await?;
                    }
                }
                AgentRunStep::CallTools { calls } => {
                    self.store
                        .save_run_checkpoint(id, &checkpoint)
                        .await?;
                    let records = self
                        .store
                        .tool_call_records(id, checkpoint.turn())
                        .await?;
                    if records.len() != calls.len() {
                        return Err(Error::State("模型工具调用与保存记录不一致"));
                    }
                    let mut results = Vec::new();
                    let mut card_requested = false;
                    for (pending, record) in calls.iter().zip(&records) {
                        let call = &pending.tool_call;
                        if pending.preresolved_result.is_some()
                            || record["call"] != serde_json::to_value(call)?
                        {
                            return Err(Error::Invalid("模型工具调用无法执行"));
                        }
                        let tool = tools
                            .iter()
                            .find(|tool| tool.definition.name == call.function.name)
                            .ok_or(Error::Invalid("调用的工具未注册"))?;
                        if record["status"] == "succeeded" {
                            results.push(serde_json::from_value::<UserContent>(
                                record["result"].clone(),
                            )?);
                            if matches!(tool.target, crate::tools::ToolTarget::Expert)
                                && tool.definition.name.as_str() == "request_expert_confirmation"
                            {
                                card_requested = true;
                            }
                            continue;
                        }
                        if card_requested {
                            results.push(
                                self.store
                                    .refuse_tool_while_waiting_for_expert_card(
                                        id, record, call,
                                    )
                                    .await?,
                            );
                            continue;
                        }
                        if record["status"] == "denied" {
                            return Err(Error::Cancelled);
                        }
                        let connector_policy = match &tool.target {
                            crate::tools::ToolTarget::Builtin
                            | crate::tools::ToolTarget::Expert => None,
                            crate::tools::ToolTarget::Mcp { connector_id, .. } => {
                                Some(self.store.active_connector_policy(connector_id).await?)
                            }
                        };
                        let requires_approval =
                            !matches!(tool.target, crate::tools::ToolTarget::Expert)
                                && (connector_policy.as_deref() == Some("approval")
                                    || run["permission_mode"] == "request_approval"
                                    || (run["permission_mode"] != "full_access"
                                        && tool.requires_approval));
                        if record["status"] != "approved"
                            && record["status"] != "waiting_workflow"
                            && record["status"] != "waiting_mcp"
                            && requires_approval
                        {
                            self.store
                                .pause_for_tool_approval(id, record, tool, call)
                                .await?;
                            self.notify();
                            return Ok(());
                        }
                        let Some(result) = self
                            .store
                            .execute_local_tool(id, record, tool, call)
                            .await?
                        else {
                            self.notify();
                            return Ok(());
                        };
                        results.push(result);
                        if matches!(tool.target, crate::tools::ToolTarget::Expert)
                            && tool.definition.name.as_str() == "request_expert_confirmation"
                        {
                            card_requested = true;
                        }
                        self.notify();
                    }
                    checkpoint.tool_results(results)?;
                    self.store
                        .save_run_checkpoint(id, &checkpoint)
                        .await?;
                    if card_requested {
                        return Ok(());
                    }
                }
                AgentRunStep::Done(_) => {
                    self.store
                        .save_run_checkpoint(id, &checkpoint)
                        .await?;
                    return Ok(());
                }
            }
        }
    }

    async fn stream_event(
        &self,
        id: &str,
        turn: usize,
        event: StreamEvent,
    ) -> Result<()> {
        let message = format!("{id}-{turn}-{}", event.part().index());
        let event = match event {
            StreamEvent::Start {
                kind: PartKind::Text,
                ..
            } => json!({ "type": "TEXT_MESSAGE_START", "messageId": message, "role": "assistant" }),
            StreamEvent::Start {
                kind: PartKind::Reasoning,
                ..
            } => json!({ "type": "REASONING_MESSAGE_START", "messageId": message }),
            StreamEvent::Text { text, .. } => {
                json!({ "type": "TEXT_MESSAGE_CONTENT", "messageId": message, "delta": text })
            }
            StreamEvent::Reasoning { text, .. } => {
                json!({ "type": "REASONING_MESSAGE_CONTENT", "messageId": message, "delta": text })
            }
            StreamEvent::Start {
                kind: PartKind::ToolCall,
                name,
                ..
            } => {
                let name = name.ok_or(Error::State("模型工具调用缺少名称"))?;
                json!({ "type": "TOOL_CALL_START", "toolCallId": message, "toolCallName": self.store.tool_display_name(id, &name).await? })
            }
            StreamEvent::Arguments { json, .. } => {
                json!({ "type": "TOOL_CALL_ARGS", "toolCallId": message, "delta": json })
            }
            StreamEvent::End {
                part,
                content: AssistantContent::ToolCall(call),
            } => {
                self.store
                    .save_tool_part(id, turn, part.index(), &call)
                    .await?;
                json!({ "type": "TOOL_CALL_END", "toolCallId": message })
            }
            StreamEvent::End {
                content: rig::message::AssistantContent::Text(_),
                ..
            } => json!({ "type": "TEXT_MESSAGE_END", "messageId": message }),
            StreamEvent::End {
                content: rig::message::AssistantContent::Reasoning(_),
                ..
            } => json!({ "type": "REASONING_MESSAGE_END", "messageId": message }),
            _ => return Err(Error::Invalid("调用的模型输出尚未注册")),
        };
        self.emit(id, event).await
    }
}
