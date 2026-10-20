pub(crate) use crate::desktop_state::AppState;
use crate::{Error, LocalRequest, local_api::text};
use serde::Serialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Serialize)]
pub(crate) struct CommandError {
    pub status: u16,
    pub code: i64,
    pub message: String,
}

impl From<Error> for CommandError {
    fn from(error: Error) -> Self {
        match error {
            Error::Remote {
                status,
                code,
                message,
            } => Self {
                status,
                code,
                message,
            },
            Error::Invalid(message) => Self {
                status: 400,
                code: 0,
                message: message.to_owned(),
            },
            Error::NotFound => Self {
                status: 404,
                code: 0,
                message: error.to_string(),
            },
            Error::Http(error) => Self {
                status: 0,
                code: 0,
                message: format!("无法连接平台：{error}"),
            },
            error => Self {
                status: 500,
                code: 0,
                message: error.to_string(),
            },
        }
    }
}

#[tauri::command]
pub(crate) async fn studio_request(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    request: LocalRequest,
) -> std::result::Result<Value, CommandError> {
    let platform = state.platform()?;
    if let Some(action) = request.path.strip_prefix("/api/v1/studio/auth/") {
        return Ok(state
            .auth_request(&app, &request.method, action, request.body)
            .await?);
    }
    if request.method == "DELETE" && request.path == "/api/v1/studio/sessions" {
        state
            .clear_sessions(
                request
                    .body
                    .as_ref()
                    .and_then(|body| body.get("confirmation")),
            )
            .await?;
        return Ok(Value::Null);
    }
    let access = state.access().await?;
    let account = &access.account;
    let previous = platform.cached_identity()?;
    let result = access
        .execute(async {
            if let Some(response) = platform
                .asset_publication_request(&state.store, &request)
                .await?
            {
                return Ok(response);
            }
            if request.method == "GET"
                && [
                    "/api/v1/studio/skill-market",
                    "/api/v1/studio/expert-market",
                    "/api/v1/studio/workflow-market",
                ]
                .iter()
                .any(|prefix| {
                    request.path == *prefix
                        || request.path.starts_with(&format!("{prefix}/"))
                        || request.path.starts_with(&format!("{prefix}?"))
                })
            {
                return platform.request_catalog(&request.path).await;
            }
            if request.method == "POST" && request.path == "/api/v1/studio/messages" {
                let input = request.body.ok_or(Error::Invalid("缺少消息内容"))?;
                platform.prepare_capabilities(&state.store, &input).await?;
                let turn = state.store.submit_message(account, input).await?;
                state.start_session(account.clone(), text(&turn["session"], "id")?.to_owned());
                return Ok(turn);
            }
            if (request.method == "POST" && request.path == "/api/v1/studio/experts")
                || (request.method == "PATCH"
                    && request
                        .path
                        .strip_prefix("/api/v1/studio/experts/")
                        .is_some_and(|id| !id.is_empty() && !id.contains('/')))
            {
                platform
                    .prepare_expert_save(
                        &state.store,
                        request
                            .body
                            .as_ref()
                            .ok_or(Error::Invalid("缺少专家内容"))?,
                    )
                    .await?;
            }
            if request.method == "POST"
                && let Some(id) = request
                    .path
                    .strip_prefix("/api/v1/studio/runs/")
                    .and_then(|path| path.strip_suffix("/cancel"))
            {
                state.runtime.cancel(account, id).await?;
                return Ok(Value::Null);
            }
            if request.method == "POST" && request.path.starts_with("/api/v1/studio/approvals/") {
                let run = state.api.request(account, request).await?;
                state.start_session(account.clone(), text(&run, "session_id")?.to_owned());
                return Ok(Value::Null);
            }
            if request.method == "POST"
                && request.path.starts_with("/api/v1/studio/expert-cards/")
                && request.path.ends_with("/response")
            {
                let run = state.api.request(account, request).await?;
                state.start_session(account.clone(), text(&run, "session_id")?.to_owned());
                return Ok(run);
            }
            state.api.request(account, request).await
        })
        .await;
    state.check_identity(&app, previous, false).await?;
    Ok(result?)
}

#[tauri::command]
pub(crate) async fn studio_run(
    state: tauri::State<'_, AppState>,
    request: Value,
) -> std::result::Result<Value, CommandError> {
    let access = state.access().await?;
    let account = &access.account;
    access.execute(async {
    let session = text(&request, "threadId")?;
    if let Some(resume) = request.get("resume").filter(|resume| !resume.is_null() && resume.as_array().is_none_or(|entries| !entries.is_empty())) {
        let entries = resume.as_array().filter(|entries| entries.len() == 1).ok_or(Error::Invalid("需要提供当前待处理事项的答复"))?;
        let entry = &entries[0];
        let run = state.store.resolve_run_interrupt(account, session, entry).await?;
        state.start_session(account.clone(), session.to_owned());
        return Ok(run);
    }
    if let Some(id) = request["attachRunId"].as_str() {
        let run = state.store.get_run(account, id).await?;
        if run["session_id"] != session { return Err(Error::Invalid("运行不属于当前会话")); }
        state.start_session(account.clone(), session.to_owned());
        return Ok(run);
    }
    let message = request["messages"].as_array().and_then(|messages| messages.iter().rev().find(|message| message["role"] == "user"))
        .ok_or(Error::Invalid("缺少用户消息"))?;
    let config = &request["forwardedProps"]["runConfig"];
    let input = json!({
        "session_id": session, "request_id": text(&request, "requestId")?, "text": text(message, "content")?,
        "model_config_id": config["modelConfigId"], "locale": config["locale"], "permission_mode": config["permissionMode"],
        "parts": config["messageParts"], "skill_ids": config["selectedSkillIds"], "assets": config["selectedAssets"],
        "expert_id": config["expertId"], "expert_version": config["expertVersion"],
    });
    state.platform()?.prepare_capabilities(&state.store, &input).await?;
    let turn = state.store.submit_message(account, input).await?;
    state.start_session(account.clone(), session.to_owned());
    Ok(turn["run"].clone())
    }).await.map_err(CommandError::from)
}

#[tauri::command]
pub(crate) async fn studio_subscribe(
    state: tauri::State<'_, AppState>,
    subscription_id: String,
    run_id: String,
    after_sequence: i64,
    on_event: tauri::ipc::Channel<Value>,
) -> std::result::Result<(), CommandError> {
    let access = state.access().await?;
    let account = access.account.clone();
    let run = access
        .execute(state.store.get_run(&account, &run_id))
        .await?;
    if subscription_id.is_empty() {
        return Err(Error::Invalid("订阅标识不能为空").into());
    }
    let cancellation = CancellationToken::new();
    {
        let mut subscriptions = state.subscriptions.lock().await;
        if subscriptions.contains_key(&subscription_id) {
            return Err(Error::Invalid("订阅标识已被使用").into());
        }
        subscriptions.insert(
            subscription_id.clone(),
            (account.clone(), cancellation.clone()),
        );
    }
    if on_event
        .send(json!({ "type": "SUBSCRIPTION_READY" }))
        .is_err()
    {
        state.subscriptions.lock().await.remove(&subscription_id);
        return Err(Error::State("运行事件订阅已关闭").into());
    }
    if run["status"] == "queued" {
        state.start_session(account.clone(), text(&run, "session_id")?.to_owned());
    }
    drop(access);
    let after_sequence =
        after_sequence.max(run["subscription_after_sequence"].as_i64().unwrap_or(0));
    let result = state
        .runtime
        .forward_events(
            &account,
            &run_id,
            after_sequence,
            on_event.clone(),
            cancellation,
        )
        .await;
    state.subscriptions.lock().await.remove(&subscription_id);
    result?;
    on_event
        .send(json!({ "type": "SUBSCRIPTION_FINISHED" }))
        .map_err(|_| Error::State("运行事件订阅已关闭"))?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn studio_unsubscribe(
    state: tauri::State<'_, AppState>,
    subscription_id: String,
) -> std::result::Result<(), CommandError> {
    let subscriptions = state.subscriptions.lock().await;
    if let Some((_, cancellation)) = subscriptions.get(&subscription_id) {
        cancellation.cancel();
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn studio_upload(
    state: tauri::State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> std::result::Result<Value, CommandError> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(Error::Invalid("文件内容必须使用二进制传输").into());
    };
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(Error::Invalid("文件超过 256 MiB").into());
    }
    let metadata = request
        .headers()
        .get("x-pixoma-upload")
        .ok_or(Error::Invalid("缺少文件信息"))?
        .to_str()
        .map_err(|_| Error::Invalid("文件信息编码无效"))?;
    if metadata.len() > 16384 {
        return Err(Error::Invalid("文件信息超过大小限制").into());
    }
    let metadata = percent_encoding::percent_decode_str(metadata)
        .decode_utf8()
        .map_err(|_| Error::Invalid("文件信息编码无效"))?;
    let metadata: Value =
        serde_json::from_str(&metadata).map_err(|_| Error::Invalid("文件信息格式无效"))?;
    let access = state.access().await?;
    Ok(access
        .execute(state.store.upload_asset(&access.account, metadata, bytes))
        .await?)
}

#[tauri::command]
pub(crate) async fn studio_inspect_skill(
    state: tauri::State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> std::result::Result<Value, CommandError> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(Error::Invalid("技能压缩文件必须使用二进制传输").into());
    };
    let access = state.access().await?;
    Ok(access
        .execute(async { crate::Store::inspect_skill_zip(bytes) })
        .await?)
}

#[tauri::command]
pub(crate) async fn studio_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    project_asset_ids: Vec<String>,
) -> std::result::Result<bool, CommandError> {
    use std::io::Write;
    use tauri_plugin_dialog::DialogExt;

    let access = state.access().await?;
    let export = access
        .execute(
            state
                .store
                .export_assets(&access.account, &project_asset_ids),
        )
        .await?;
    let destination = app
        .dialog()
        .file()
        .set_file_name(&export.name)
        .blocking_save_file();
    let Some(destination) = destination else {
        return Ok(false);
    };
    if access.cancellation.is_cancelled() {
        return Err(Error::Cancelled.into());
    }
    if state.platform()?.account_scope().await? != access.account {
        return Err(Error::Invalid("账号已更改，请重新导出").into());
    }
    let destination = destination
        .into_path()
        .map_err(|_| Error::Invalid("保存位置无效"))?;
    let parent = destination.parent().ok_or(Error::Invalid("保存位置无效"))?;
    let mut output = tempfile::NamedTempFile::new_in(parent).map_err(Error::from)?;
    output.write_all(&export.bytes).map_err(Error::from)?;
    output.as_file().sync_all().map_err(Error::from)?;
    output
        .persist(destination)
        .map_err(|error| Error::Io(error.error))?;
    Ok(true)
}
