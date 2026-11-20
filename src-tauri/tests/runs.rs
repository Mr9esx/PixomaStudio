use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::json;
use std::path::PathBuf;

#[tokio::test]
async fn message_requests_create_a_local_session_and_preserve_the_request_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("messages-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    api.request(LocalRequest { method: "POST".to_owned(), path: "/api/v1/studio-operations/models".to_owned(), body: Some(json!({
        "name": "本地模型", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:1/v1", "model": "local-model", "enabled": true, "agent_enabled": true, "default": true,
        "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 }, "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
    })) }).await?;
    let project = store.create_project("创作项目").await?;
    let body = json!({ "request_id": "message-request", "text": "撰写文稿", "project_id": project.id, "locale": "zh", "permission_mode": "request_approval", "parts": [{ "type": "text", "text": "撰写文稿" }] });
    let turn = api
        .request(
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio/messages".to_owned(),
                body: Some(body.clone()),
            },
        )
        .await?;
    let repeated = api
        .request(
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio/messages".to_owned(),
                body: Some(body.clone()),
            },
        )
        .await?;
    assert_eq!(turn["run"]["id"], repeated["run"]["id"]);
    assert_eq!(turn["session"]["project_id"], project.id);
    assert_eq!(turn["run"]["status"], "queued");
    let mut unavailable = body.clone();
    unavailable["request_id"] = json!("unavailable-model");
    unavailable["model_config_id"] = json!("missing-model");
    assert!(
        api.request(
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio/messages".to_owned(),
                body: Some(unavailable)
            }
        )
        .await
        .is_err()
    );
    assert_eq!(store.list_sessions(None, false, 100, 0).await?.len(), 1);
    let source_session = turn["session"]["id"].as_str().ok_or("会话标识缺失")?;
    let owned = store
        .create_text_asset(
            source_session,
            "引用文档",
            "保留选定的资料",
            "owned-document",
        )
        .await?;
    let selected = store.submit_message(json!({
        "request_id": "owned-reference", "text": "读取资料", "locale": "zh", "permission_mode": "auto_approve",
        "parts": [{ "type": "text", "text": "读取资料" }, { "type": "asset_ref", "asset_id": owned.id, "asset_version_id": owned.versions[0].id, "name": owned.name }],
        "assets": [{ "assetId": owned.id, "assetVersionId": owned.versions[0].id }],
    })).await?;
    let selected_session = selected["session"]["id"].as_str().ok_or("会话标识缺失")?;
    assert_eq!(
        selected["message"]["parts"]
            .as_array()
            .ok_or("消息内容缺失")?
            .len(),
        2
    );
    assert_eq!(selected["message"]["parts"][0]["text"], "读取资料");
    let assets = store.list_session_assets(selected_session).await?;
    assert_eq!(assets.len(), 1);
    assert_eq!(
        assets[0]
            .usages
            .iter()
            .filter(|usage| usage["session_id"] == selected_session)
            .count(),
        1
    );
    let events = api
        .request(
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions/{selected_session}/context/events"),
                body: None,
            },
        )
        .await?;
    assert_eq!(
        events["events"].as_array().ok_or("上下文事件缺失")?.len(),
        1
    );
    let mut changed = body;
    changed["parts"] = json!([{ "type": "text", "text": "其他文稿" }]);
    assert!(
        api.request(
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio/messages".to_owned(),
                body: Some(changed)
            }
        )
        .await
        .is_err()
    );
    let run_id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let run = api
        .request(
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/runs/{run_id}"),
                body: None,
            },
        )
        .await?;
    assert_eq!(run["id"], run_id);
    assert!(store.get_run("missing-run").await.is_err());
    let other_session = store.create_session("private-document").await?;
    let other_asset = store
        .create_text_asset(
            &other_session.id,
            "另一份文档",
            "另一会话的资料",
            "private-document",
        )
        .await?;
    assert!(store.submit_message(json!({
        "request_id": "unowned-reference", "text": "读取所选文档", "locale": "zh", "permission_mode": "auto_approve",
        "parts": [{ "type": "asset_ref", "asset_id": owned.id, "asset_version_id": other_asset.versions[0].id, "name": owned.name }],
    })).await.is_err());
    assert_eq!(store.list_sessions(None, false, 100, 0).await?.len(), 3);
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn queued_runs_keep_real_rig_checkpoints_and_event_order_after_reopening()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("runs-").tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let model = api.request(LocalRequest { method: "POST".to_owned(), path: "/api/v1/studio-operations/models".to_owned(), body: Some(json!({
        "name": "本地模型", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:11434/v1", "model": "local-model", "enabled": true, "agent_enabled": true, "default": true,
        "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 }, "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
    })) }).await?;
    let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
    let session = store.create_session("session").await?;
    let turn = store
        .queue_run(
            &session.id,
            "request",
            model_id,
            "请读取项目文稿",
            "request_approval",
        )
        .await?;
    let run_id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let repeated = store
        .queue_run(
            &session.id,
            "request",
            model_id,
            "请读取项目文稿",
            "request_approval",
        )
        .await?;
    assert_eq!(turn["run"]["id"], repeated["run"]["id"]);
    assert!(
        store
            .queue_run(
                &session.id,
                "request",
                model_id,
                "其他内容",
                "request_approval"
            )
            .await
            .is_err()
    );
    assert!(store.get_run("missing-run").await.is_err());
    let (left, right) = tokio::join!(
        store.queue_run(
            &session.id,
            "concurrent",
            model_id,
            "并发消息",
            "request_approval"
        ),
        store.queue_run(
            &session.id,
            "concurrent",
            model_id,
            "并发消息",
            "request_approval"
        ),
    );
    let concurrent = left?;
    assert_eq!(concurrent["run"]["id"], right?["run"]["id"]);
    let concurrent_id = concurrent["run"]["id"].as_str().ok_or("运行标识缺失")?;
    assert!(!store.claim_run(concurrent_id).await?);
    assert!(store.claim_run(run_id).await?);
    assert!(!store.claim_run(concurrent_id).await?);
    let (first, second) = tokio::join!(
        store.append_run_event(
            run_id,
            json!({ "type": "CUSTOM", "name": "first" })
        ),
        store.append_run_event(
            run_id,
            json!({ "type": "CUSTOM", "name": "second" })
        ),
    );
    let mut sequences = [
        first?["sequence"].as_u64().ok_or("事件序号缺失")?,
        second?["sequence"].as_u64().ok_or("事件序号缺失")?,
    ];
    sequences.sort();
    assert_eq!(sequences, [1, 2]);
    store.close().await;
    let reopened = Store::open(&database).await?;
    let mut checkpoint = reopened.run_checkpoint(run_id).await?;
    assert!(matches!(
        checkpoint.next_step()?,
        rig::run::AgentRunStep::CallModel { .. }
    ));
    let events = reopened.run_events_after(run_id, 1).await?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["sequence"], 2);
    assert_eq!(
        reopened.get_run(run_id).await?["status"],
        "running"
    );
    assert_eq!(reopened.recover_interrupted_runs().await?, 1);
    assert_eq!(
        reopened.get_run(run_id).await?["status"],
        "failed"
    );
    assert_eq!(
        reopened.get_run(run_id).await?["error_code"],
        "runtime_interrupted"
    );
    assert!(reopened.claim_run(concurrent_id).await?);
    let detail = reopened.session_detail(&session.id).await?;
    assert_eq!(
        detail["messages"][0]["content"][0]["text"],
        "请读取项目文稿"
    );
    reopened.close().await;
    Ok(())
}
