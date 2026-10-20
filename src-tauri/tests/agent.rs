use pixoma_studio::{AgentRuntime, LocalApi, LocalRequest, Store};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

struct TestCredential(keyring::Entry);

impl Drop for TestCredential {
    fn drop(&mut self) {
        self.0.delete_credential().expect("删除测试模型凭据失败");
    }
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接支持图片的真实模型服务"]
async fn real_model_reads_image_pixels_with_a_budget_based_on_dimensions()
-> Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    let mut configuration: serde_json::Value =
        serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    configuration["capabilities"]["tools"] = json!(false);
    configuration["capabilities"]["vision"] = json!(true);
    let deepseek_chat = configuration["protocol"] == "openai_chat_compatible"
        && configuration["model"]
            .as_str()
            .is_some_and(|model| model.starts_with("deepseek-"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("real-image-input-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let mut store = Store::open(&database).await?;
    let account = "image-input-check";
    let model = LocalApi::new(store.clone())
        .request(
            account,
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration.clone()),
            },
        )
        .await?;
    let model = model["id"].as_str().ok_or("模型标识缺失")?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind(account)
            .bind(model)
            .fetch_one(&connection)
            .await?;
    let _credential = TestCredential(keyring::Entry::new("com.pixoma.studio", &reference)?);
    connection.close().await;
    let session = store.create_session(account, "image").await?;
    let salt = uuid::Uuid::new_v4();
    let image = image::RgbImage::from_fn(320, 320, |x, y| {
        let mut hash = Sha256::new();
        hash.update(salt.as_bytes());
        hash.update(x.to_le_bytes());
        hash.update(y.to_le_bytes());
        let value = hash.finalize();
        let mut pixel = [value[0] % 24, value[1] % 24, value[2] % 24];
        pixel[if y < 160 { 0 } else { 2 }] = 220 + value[3] % 36;
        image::Rgb(pixel)
    });
    let source = directory.path().join("reference.png");
    image.save(&source)?;
    let bytes = std::fs::read(&source)?;
    assert!(bytes.len() > 100_000);
    let asset = store.import_asset(account, &session.id, &source).await?;
    configuration["api_key"] = json!("");
    for streaming in [true, false] {
        configuration["capabilities"]["streaming"] = json!(streaming);
        LocalApi::new(store.clone())
            .request(
                account,
                LocalRequest {
                    method: "PATCH".to_owned(),
                    path: format!("/api/v1/studio-operations/models/{model}"),
                    body: Some(configuration.clone()),
                },
            )
            .await?;
        let session = store
            .create_session(account, &format!("image-{streaming}"))
            .await?;
        let submitted = store.submit_message(account, json!({
            "session_id": session.id, "request_id": format!("image-{streaming}"), "text": "当前图片的上半部分和下半部分各是什么颜色？只按顺序回答两个中文颜色名称。",
            "permission_mode": "auto_approve", "assets": [{ "asset_id": asset.id, "asset_version_id": asset.versions[0].id }],
        })).await?;
        let run = submitted["run"]["id"].as_str().ok_or("运行标识缺失")?;
        let outcome = AgentRuntime::new(store.clone())
            .execute(account, run)
            .await?;
        assert_eq!(
            outcome["status"], "succeeded",
            "{}",
            outcome["error_message"]
        );
        if deepseek_chat {
            let connection =
                sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
            let request: String = sqlx::query_scalar("SELECT request FROM model_requests WHERE account_id=? AND run_id=? ORDER BY created_at DESC LIMIT 1")
                .bind(account).bind(run).fetch_one(&connection).await?;
            let request: serde_json::Value = serde_json::from_str(&request)?;
            assert_eq!(request["additional_params"]["thinking"]["type"], "disabled");
            connection.close().await;
        }
        let output: String = store
            .run_events_after(account, run, 0)
            .await?
            .iter()
            .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
            .filter_map(|event| event["payload"]["delta"].as_str())
            .collect();
        let red = output
            .find('红')
            .ok_or_else(|| format!("模型没有识别上半部分的颜色：{output}"))?;
        let blue = output
            .find('蓝')
            .ok_or_else(|| format!("模型没有识别下半部分的颜色：{output}"))?;
        assert!(red < blue, "模型颜色顺序不一致：{output}");
        let request = LocalApi::new(store.clone())
            .request(
                account,
                LocalRequest {
                    method: "GET".to_owned(),
                    path: format!("/api/v1/studio/sessions/{}/context/current", session.id),
                    body: None,
                },
            )
            .await?;
        assert!(request["projected_tokens"].as_u64().ok_or("输入估算缺失")? < 24_000);
        assert!(request["input_tokens"].as_u64().ok_or("真实输入用量缺失")? < 24_000);
        let parts = request["parts"].as_array().ok_or("上下文内容缺失")?;
        assert!(parts.iter().any(|part| {
            part["label"] == "图片资料"
                && part["estimated_tokens"]
                    .as_u64()
                    .is_some_and(|tokens| tokens > 0 && tokens < 5000)
        }));
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(
            store
                .read_asset(account, &asset.id, &asset.versions[0].id)
                .await?,
            bytes
        );
        assert!(store.run_checkpoint(account, run).await?.full_history().iter().any(|message| {
            matches!(message, rig::message::Message::User { content } if content.iter().any(|part| matches!(part, rig::message::UserContent::Image(_))))
        }));
    }
    store.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接支持思考的真实模型服务"]
async fn real_model_reasoning_is_saved_for_streaming_and_complete_responses()
-> Result<(), Box<dyn std::error::Error>> {
    let mut configuration: serde_json::Value =
        serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    configuration["thinking"] = json!({ "enabled": true, "effort": "low", "budget_tokens": 1024 });
    configuration["limits"]["max_output_tokens"] = json!(4096);
    configuration["capabilities"]["tools"] = json!(false);
    configuration["capabilities"]["vision"] = json!(true);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("real-reasoning-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let mut store = Store::open(&database).await?;
    let account = "reasoning-check";
    let api = LocalApi::new(store.clone());
    let model = api
        .request(
            account,
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration.clone()),
            },
        )
        .await?;
    let model = model["id"].as_str().ok_or("模型标识缺失")?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind(account)
            .bind(model)
            .fetch_one(&connection)
            .await?;
    let _credential = TestCredential(keyring::Entry::new("com.pixoma.studio", &reference)?);
    connection.close().await;
    drop(api);
    configuration["api_key"] = json!("");
    for streaming in [true, false] {
        configuration["capabilities"]["streaming"] = json!(streaming);
        LocalApi::new(store.clone())
            .request(
                account,
                LocalRequest {
                    method: "PATCH".to_owned(),
                    path: format!("/api/v1/studio-operations/models/{model}"),
                    body: Some(configuration.clone()),
                },
            )
            .await?;
        let session = store
            .create_session(account, &format!("reasoning-{streaming}"))
            .await?;
        let turn = store
            .queue_run(
                account,
                &session.id,
                &format!("reasoning-{streaming}"),
                model,
                "计算 17×23，核对计算结果后仅回复最终数字。",
                "auto_approve",
            )
            .await?;
        let run = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
        let outcome = AgentRuntime::new(store.clone())
            .execute(account, run)
            .await?;
        assert_eq!(
            outcome["status"], "succeeded",
            "{}",
            outcome["error_message"]
        );
        let events = store.run_events_after(account, run, 0).await?;
        let reasoning: String = events
            .iter()
            .filter(|event| event["type"] == "REASONING_MESSAGE_CONTENT")
            .filter_map(|event| event["payload"]["delta"].as_str())
            .collect();
        let output: String = events
            .iter()
            .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
            .filter_map(|event| event["payload"]["delta"].as_str())
            .collect();
        assert!(
            !reasoning.trim().is_empty(),
            "模型没有返回思考内容，streaming={streaming}"
        );
        assert!(output.contains("391"), "模型计算结果缺失：{output}");
        assert!(
            events
                .iter()
                .any(|event| event["type"] == "REASONING_MESSAGE_START")
        );
        assert!(
            events
                .iter()
                .any(|event| event["type"] == "REASONING_MESSAGE_END")
        );
        assert!(store.run_checkpoint(account, run).await?.full_history().iter().any(|message| {
            matches!(message, rig::message::Message::Assistant(assistant) if assistant.content.iter().any(|content| {
                matches!(content, rig::message::AssistantContent::Reasoning(saved) if saved.text == reasoning)
            }))
        }));
        store.close().await;
        store = Store::open(&database).await?;
        let restored: String = store
            .run_events_after(account, run, 0)
            .await?
            .iter()
            .filter(|event| event["type"] == "REASONING_MESSAGE_CONTENT")
            .filter_map(|event| event["payload"]["delta"].as_str())
            .collect();
        assert_eq!(restored, reasoning);
    }
    store.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接真实模型服务"]
async fn real_model_continues_the_saved_history_after_an_output_limit()
-> Result<(), Box<dyn std::error::Error>> {
    let mut configuration: serde_json::Value =
        serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    configuration["capabilities"]["tools"] = json!(false);
    configuration["limits"]["max_output_tokens"] = json!(128);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("limited-history-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let mut store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let account = "limited-history";
    let model = api
        .request(
            account,
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration.clone()),
            },
        )
        .await?;
    let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind(account)
            .bind(model_id)
            .fetch_one(&connection)
            .await?;
    let _credential = TestCredential(keyring::Entry::new("com.pixoma.studio", &reference)?);
    connection.close().await;
    let marker = uuid::Uuid::new_v4().to_string();
    let first = store.submit_message(account, json!({
        "request_id": "limited-output", "text": format!("本次会话编号为 {marker}，请记住编号。现在仅输出包含数字 1 到 500 的完整 JSON 数组，不要省略、解释或输出编号。"),
        "permission_mode": "auto_approve",
    })).await?;
    let run = first["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let session = first["session"]["id"].as_str().ok_or("会话标识缺失")?;
    let outcome = AgentRuntime::new(store.clone())
        .execute(account, run)
        .await?;
    assert_eq!(outcome["status"], "failed");
    assert!(
        outcome["error_message"]
            .as_str()
            .ok_or("错误说明缺失")?
            .contains("输出限制"),
        "{}",
        outcome["error_message"]
    );
    let limited_request = api
        .request(
            account,
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions/{session}/context/current"),
                body: None,
            },
        )
        .await?;
    assert_eq!(limited_request["status"], "failed");
    let events = store.run_events_after(account, run, 0).await?;
    assert!(
        events
            .iter()
            .any(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
    );
    let partial: String = events
        .iter()
        .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
        .filter_map(|event| event["payload"]["delta"].as_str())
        .collect();
    configuration["limits"]["max_output_tokens"] = json!(2048);
    configuration["api_key"] = json!("");
    api.request(
        account,
        LocalRequest {
            method: "PATCH".to_owned(),
            path: format!("/api/v1/studio-operations/models/{model_id}"),
            body: Some(configuration),
        },
    )
    .await?;
    drop(api);
    store.close().await;
    store = Store::open(&database).await?;
    let next = store.submit_message(account, json!({ "session_id": session, "request_id": "continued-output", "text": "刚才的会话编号是什么？仅回复完整编号。", "permission_mode": "auto_approve" })).await?;
    let run = next["run"]["id"].as_str().ok_or("后续运行标识缺失")?;
    let outcome = AgentRuntime::new(store.clone())
        .execute(account, run)
        .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let events = store.run_events_after(account, run, 0).await?;
    let output: String = events
        .iter()
        .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
        .filter_map(|event| event["payload"]["delta"].as_str())
        .collect();
    let context = LocalApi::new(store.clone())
        .request(
            account,
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions/{session}/context/current"),
                body: None,
            },
        )
        .await?;
    assert!(
        context["parts"]
            .as_array()
            .ok_or("请求内容缺失")?
            .iter()
            .any(|part| part["category"] == "user_message"
                && part["content"]
                    .as_str()
                    .is_some_and(|content| content.contains(&marker))),
        "后续请求没有包含上一轮用户消息"
    );
    assert!(
        output.contains(&marker),
        "模型没有返回上一轮会话编号：{output}"
    );
    let history = store.run_checkpoint(account, run).await?.full_history();
    assert!(history.iter().any(|message| matches!(message, rig::message::Message::Assistant(assistant) if assistant.content.iter().any(|content| matches!(content, rig::message::AssistantContent::Text(text) if text.text == partial)))), "后续模型请求没有保留上一轮已生成的正文");
    store.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接真实模型服务"]
async fn real_model_streams_text_and_reuses_persisted_history()
-> Result<(), Box<dyn std::error::Error>> {
    let configuration: serde_json::Value =
        serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("real-agent-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let model = api
        .request(
            "real-model-check",
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration),
            },
        )
        .await?;
    let model = model["id"].as_str().ok_or("模型标识缺失")?;
    let session = store
        .create_session("real-model-check", "conversation")
        .await?;
    let runtime = AgentRuntime::new(store.clone());
    for (request, prompt, expected) in [
        (
            "first",
            "请记住本次对话的编号为 PX73916，仅回复该编号。",
            "PX73916",
        ),
        ("second", "刚刚的编号是什么？仅回复编号。", "PX73916"),
    ] {
        let turn = store
            .queue_run(
                "real-model-check",
                &session.id,
                request,
                model,
                prompt,
                "auto_approve",
            )
            .await?;
        let id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
        let outcome = runtime.execute("real-model-check", id).await?;
        assert_eq!(
            outcome["status"], "succeeded",
            "{}",
            outcome["error_message"]
        );
        let events = store.run_events_after("real-model-check", id, 0).await?;
        assert_eq!(events.first().ok_or("运行事件缺失")?["type"], "RUN_STARTED");
        assert_eq!(events.last().ok_or("结束事件缺失")?["type"], "RUN_FINISHED");
        let text = events
            .iter()
            .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
            .filter_map(|event| event["payload"]["delta"].as_str())
            .collect::<String>();
        assert!(text.contains(expected), "模型未返回当前对话编号");
        assert!(
            events
                .iter()
                .any(|event| event["payload"]["name"] == "usage")
        );
        assert!(
            !store
                .run_checkpoint("real-model-check", id)
                .await?
                .full_history()
                .is_empty()
        );
    }
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind("real-model-check")
            .bind(model)
            .fetch_one(&connection)
            .await?;
    keyring::Entry::new("com.pixoma.studio", &reference)?.delete_credential()?;
    connection.close().await;
    store.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接真实模型服务"]
async fn real_model_creates_a_document_using_the_local_tool()
-> Result<(), Box<dyn std::error::Error>> {
    let configuration: serde_json::Value =
        serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("real-tools-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let mut store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let model = api
        .request(
            "real-tool-check",
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration),
            },
        )
        .await?;
    let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind("real-tool-check")
            .bind(model_id)
            .fetch_one(&connection)
            .await?;
    let _credential = TestCredential(keyring::Entry::new("com.pixoma.studio", &reference)?);
    connection.close().await;
    drop(api);
    let session = store
        .create_session("real-tool-check", "document-tool")
        .await?;
    let turn = store.queue_run("real-tool-check", &session.id, "document-tool", model_id,
        "请调用 create_text_asset 工具创建一个名为 PX工具检查 的 Markdown 文档，正文严格为 PX73916。完成后仅回复完成。", "auto_approve").await?;
    let id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let outcome = AgentRuntime::new(store.clone())
        .execute("real-tool-check", id)
        .await?;
    let assets = store
        .list_session_assets("real-tool-check", &session.id)
        .await?;
    let events = store.run_events_after("real-tool-check", id, 0).await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    assert_eq!(assets.len(), 1);
    assert_eq!(
        store
            .read_asset("real-tool-check", &assets[0].id, &assets[0].versions[0].id)
            .await?,
        b"PX73916"
    );
    assert!(
        events
            .iter()
            .any(|event| event["type"] == "TOOL_CALL_START")
    );
    assert!(events.iter().any(|event| event["type"] == "TOOL_CALL_ARGS"));
    assert!(
        events
            .iter()
            .any(|event| event["type"] == "TOOL_CALL_RESULT")
    );
    let detail = store.session_detail("real-tool-check", &session.id).await?;
    let tool_message = detail["transcript"]["messages"]
        .as_array()
        .ok_or("会话记录缺失")?
        .iter()
        .find(|message| {
            message["toolCalls"]
                .as_array()
                .is_some_and(|calls| !calls.is_empty())
        })
        .ok_or("会话记录缺少工具调用")?;
    let tool_event = events
        .iter()
        .find(|event| event["type"] == "TOOL_CALL_START")
        .ok_or("工具开始事件缺失")?;
    assert_eq!(
        tool_message["toolCalls"][0]["id"],
        tool_event["payload"]["toolCallId"]
    );
    let mut expected_assets = 1;
    for (request, approved, edited) in [
        ("approve", true, false),
        ("deny", false, false),
        ("edit", true, true),
    ] {
        let turn = store.queue_run("real-tool-check", &session.id, request, model_id,
            &format!("请调用 create_text_asset 工具创建一个名为 PX审批{request} 的 Markdown 文档，正文严格为 PX73916。完成后仅回复完成。"), "request_approval").await?;
        let id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
        let waiting = AgentRuntime::new(store.clone())
            .execute("real-tool-check", id)
            .await?;
        assert_eq!(
            waiting["status"], "waiting_approval",
            "{}",
            waiting["error_message"]
        );
        assert_eq!(
            store
                .list_session_assets("real-tool-check", &session.id)
                .await?
                .len(),
            expected_assets
        );
        loop {
            let detail = store.session_detail("real-tool-check", &session.id).await?;
            let pending = &detail["pending_approvals"][0];
            if pending["metadata"]["toolName"] == "create_text_asset" {
                break;
            }
            let name = pending["metadata"]["toolName"]
                .as_str()
                .ok_or("审批缺少工具名称")?;
            assert!(
                ["list_assets", "read_asset"].contains(&name),
                "创建文档前请求了其他写入操作：{name}"
            );
            store
                .resolve_tool_approval(
                    "real-tool-check",
                    pending["id"].as_str().ok_or("读取审批缺失")?,
                    true,
                    None,
                )
                .await?;
            let waiting = AgentRuntime::new(store.clone())
                .execute("real-tool-check", id)
                .await?;
            assert_eq!(
                waiting["status"], "waiting_approval",
                "{}",
                waiting["error_message"]
            );
            assert_eq!(
                store
                    .list_session_assets("real-tool-check", &session.id)
                    .await?
                    .len(),
                expected_assets
            );
        }
        store.close().await;
        store = Store::open(&database).await?;
        assert_eq!(store.recover_interrupted_runs().await?, 0);
        let detail = store.session_detail("real-tool-check", &session.id).await?;
        let approval = detail["pending_approvals"][0]["id"]
            .as_str()
            .ok_or("审批记录缺失")?;
        assert!(
            store
                .resolve_tool_approval("account-b", approval, true, None)
                .await
                .is_err()
        );
        assert!(
            store
                .resolve_tool_approval(
                    "real-tool-check",
                    approval,
                    true,
                    Some(json!({ "content": 3 }))
                )
                .await
                .is_err()
        );
        let edited_arguments =
            edited.then(|| json!({ "name": "PX修改参数", "content": "PX已修改" }));
        store
            .resolve_tool_approval(
                "real-tool-check",
                approval,
                approved,
                edited_arguments.clone(),
            )
            .await
            .map_err(|error| {
                format!(
                    "审批实际工具 {} 失败：{error}，参数：{}",
                    detail["pending_approvals"][0]["metadata"]["toolName"],
                    detail["pending_approvals"][0]["metadata"]["arguments"]
                )
            })?;
        store
            .resolve_tool_approval("real-tool-check", approval, approved, edited_arguments)
            .await?;
        assert!(
            store
                .resolve_tool_approval("real-tool-check", approval, !approved, None)
                .await
                .is_err()
        );
        let outcome = AgentRuntime::new(store.clone())
            .execute("real-tool-check", id)
            .await?;
        assert_eq!(
            outcome["status"],
            if approved { "succeeded" } else { "cancelled" },
            "{}",
            outcome["error_message"]
        );
        expected_assets += usize::from(approved);
        let assets = store
            .list_session_assets("real-tool-check", &session.id)
            .await?;
        assert_eq!(assets.len(), expected_assets);
        if edited {
            let asset = assets
                .iter()
                .find(|asset| asset.name == "PX修改参数.md")
                .ok_or("修改参数后的文档缺失")?;
            assert_eq!(
                store
                    .read_asset("real-tool-check", &asset.id, &asset.versions[0].id)
                    .await?,
                "PX已修改".as_bytes()
            );
        }
        assert!(
            store.session_detail("real-tool-check", &session.id).await?["pending_approvals"]
                .as_array()
                .ok_or("审批列表缺失")?
                .is_empty()
        );
    }
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn queued_runs_stop_before_requesting_a_disabled_model()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("disabled-model-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let mut configuration = json!({
        "name": "启停检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:1/v1", "model": "local-model",
        "enabled": true, "agent_enabled": true, "default": false,
        "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
        "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
    });
    let model = api
        .request(
            "account-a",
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration.clone()),
            },
        )
        .await?;
    let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
    for field in ["enabled", "agent_enabled"] {
        let session = store.create_session("account-a", field).await?;
        let turn = store
            .queue_run(
                "account-a",
                &session.id,
                field,
                model_id,
                "检查模型启停",
                "auto_approve",
            )
            .await?;
        let run_id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
        configuration[field] = json!(false);
        api.request(
            "account-a",
            LocalRequest {
                method: "PATCH".to_owned(),
                path: format!("/api/v1/studio-operations/models/{model_id}"),
                body: Some(configuration.clone()),
            },
        )
        .await?;
        let outcome = AgentRuntime::new(store.clone())
            .execute("account-a", run_id)
            .await?;
        assert_eq!(outcome["status"], "failed");
        assert_eq!(outcome["error_message"], "模型尚未启用 Agent 调用");
        let requests = api
            .request(
                "account-a",
                LocalRequest {
                    method: "GET".to_owned(),
                    path: format!("/api/v1/studio/sessions/{}/context/requests", session.id),
                    body: None,
                },
            )
            .await?;
        assert_eq!(requests["requests"], json!([]));
        configuration[field] = json!(true);
        api.request(
            "account-a",
            LocalRequest {
                method: "PATCH".to_owned(),
                path: format!("/api/v1/studio-operations/models/{model_id}"),
                body: Some(configuration.clone()),
            },
        )
        .await?;
    }
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn agent_request_failures_and_cancellation_are_saved_as_terminal_events()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("agent-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let model = api.request("account-a", LocalRequest { method: "POST".to_owned(), path: "/api/v1/studio-operations/models".to_owned(), body: Some(json!({
        "name": "连接检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:1/v1", "model": "local-model", "enabled": true, "agent_enabled": true, "default": true,
        "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 }, "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
    })) }).await?;
    let model = model["id"].as_str().ok_or("模型标识缺失")?;
    let session = store.create_session("account-a", "session").await?;
    let first = store
        .queue_run(
            "account-a",
            &session.id,
            "first",
            model,
            "检查连接",
            "auto_approve",
        )
        .await?;
    let first_id = first["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let runtime = AgentRuntime::new(store.clone());
    let received = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let output = received.clone();
    let channel = tauri::ipc::Channel::new(move |body| {
        let tauri::ipc::InvokeResponseBody::Json(body) = body else {
            panic!("事件需要 JSON 编码");
        };
        output
            .lock()
            .expect("事件列表无法写入")
            .push(serde_json::from_str(&body).expect("事件 JSON 无效"));
        Ok(())
    });
    let (outcome, subscription) = tokio::join!(
        runtime.execute("account-a", first_id),
        runtime.forward_events("account-a", first_id, 0, channel, CancellationToken::new()),
    );
    let outcome = outcome?;
    subscription?;
    assert_eq!(outcome["status"], "failed");
    assert!(
        outcome["error_message"]
            .as_str()
            .ok_or("错误内容缺失")?
            .contains("HttpError")
    );
    let events = store.run_events_after("account-a", first_id, 0).await?;
    assert_eq!(
        *received.lock().expect("事件列表无法读取"),
        events
            .iter()
            .map(|event| event["payload"].clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(events[0]["type"], "RUN_STARTED");
    assert_eq!(events.last().ok_or("结束事件缺失")?["type"], "RUN_ERROR");
    assert!(runtime.execute("account-b", first_id).await.is_err());
    let second = store
        .queue_run(
            "account-a",
            &session.id,
            "second",
            model,
            "取消运行",
            "auto_approve",
        )
        .await?;
    let second_id = second["run"]["id"].as_str().ok_or("运行标识缺失")?;
    runtime.cancel("account-a", second_id).await?;
    assert_eq!(
        store.get_run("account-a", second_id).await?["status"],
        "cancelled"
    );
    runtime.cancel("account-a", second_id).await?;
    assert_eq!(
        store
            .run_events_after("account-a", second_id, 0)
            .await?
            .len(),
        1
    );
    assert_eq!(
        store.run_events_after("account-a", second_id, 0).await?[0]["payload"]["outcome"]["type"],
        "cancelled"
    );
    let third = store
        .queue_run(
            "account-a",
            &session.id,
            "third",
            model,
            "重新启动后的消息",
            "auto_approve",
        )
        .await?;
    let third_id = third["run"]["id"].as_str().ok_or("运行标识缺失")?;
    assert!(store.clear_sessions("account-a").await.is_err());
    assert!(store.get_session("account-a", &session.id).await.is_ok());
    let session_id = session.id.clone();
    drop(runtime);
    store.close().await;
    let reopened = Store::open(&directory.path().join("studio.sqlite")).await?;
    assert_eq!(reopened.recover_interrupted_runs().await?, 0);
    AgentRuntime::new(reopened.clone())
        .resume_account("account-a")
        .await?;
    assert_eq!(
        reopened.get_run("account-a", third_id).await?["status"],
        "failed"
    );
    assert_eq!(
        reopened.get_run("account-a", third_id).await?["session_id"],
        session_id
    );
    reopened.clear_sessions("account-a").await?;
    assert!(
        reopened
            .get_session("account-a", &session_id)
            .await
            .is_err()
    );
    assert!(reopened.get_run("account-a", third_id).await.is_err());
    reopened.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接真实模型服务"]
async fn real_model_reads_the_selected_asset_version_from_another_session()
-> Result<(), Box<dyn std::error::Error>> {
    let mut configuration: serde_json::Value =
        serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    configuration["capabilities"]["tools"] = json!(false);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("real-context-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let model = LocalApi::new(store.clone())
        .request(
            "real-context-check",
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration),
            },
        )
        .await?;
    let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM model_configs WHERE account_id=? AND id=?")
            .bind("real-context-check")
            .bind(model_id)
            .fetch_one(&connection)
            .await?;
    let _credential = TestCredential(keyring::Entry::new("com.pixoma.studio", &reference)?);
    connection.close().await;
    let source = store.create_session("real-context-check", "source").await?;
    let expected = uuid::Uuid::new_v4().to_string();
    let retained = uuid::Uuid::new_v4().to_string();
    let replaced = uuid::Uuid::new_v4().to_string();
    let asset = store
        .create_text_asset(
            "real-context-check",
            &source.id,
            "编号资料",
            &format!("审核编号：{expected}\n备用编号：{retained}"),
            "source-document",
        )
        .await?;
    store
        .update_text_asset(
            "real-context-check",
            &asset.id,
            &format!("审核编号：{replaced}"),
            "source-update",
        )
        .await?;
    let turn = store.submit_message("real-context-check", json!({
        "request_id": "selected-version", "text": "请根据所附文档仅回复审核编号。", "locale": "zh", "permission_mode": "auto_approve", "model_config_id": model_id,
        "parts": [{ "type": "asset_ref", "asset_id": asset.id, "asset_version_id": asset.versions[0].id, "name": asset.name }],
    })).await?;
    let id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let outcome = AgentRuntime::new(store.clone())
        .execute("real-context-check", id)
        .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let text = store
        .run_events_after("real-context-check", id, 0)
        .await?
        .iter()
        .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
        .filter_map(|event| event["payload"]["delta"].as_str())
        .collect::<String>();
    assert!(text.contains(&expected), "模型未读取选定的资产版本");
    assert!(!text.contains(&replaced), "模型读取了其他资产版本");
    let session_id = turn["session"]["id"].as_str().ok_or("会话标识缺失")?;
    assert!(
        store
            .list_session_assets("real-context-check", session_id)
            .await?
            .iter()
            .any(|selected| selected.id == asset.id)
    );
    let followup = store.submit_message("real-context-check", json!({
        "session_id": session_id, "request_id": "selected-version-followup", "text": "同一份资料里的备用编号是什么？仅回复备用编号。",
        "locale": "zh", "permission_mode": "auto_approve", "model_config_id": model_id,
    })).await?;
    let followup_id = followup["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let followup_outcome = AgentRuntime::new(store.clone())
        .execute("real-context-check", followup_id)
        .await?;
    assert_eq!(
        followup_outcome["status"], "succeeded",
        "{}",
        followup_outcome["error_message"]
    );
    let followup_text = store
        .run_events_after("real-context-check", followup_id, 0)
        .await?
        .iter()
        .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
        .filter_map(|event| event["payload"]["delta"].as_str())
        .collect::<String>();
    assert!(
        followup_text.contains(&retained),
        "连续对话未保留引用资料的原始版本"
    );
    let api = LocalApi::new(store.clone());
    let overview = api
        .request(
            "real-context-check",
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions/{session_id}/context"),
                body: None,
            },
        )
        .await?;
    assert_eq!(overview["turns"], 2);
    assert_eq!(overview["steps"], 2);
    assert!(
        overview["tokens"]["input"]
            .as_u64()
            .is_some_and(|tokens| tokens > 0)
    );
    let attempt = overview["current"]["attempt_id"]
        .as_str()
        .ok_or("上下文请求标识缺失")?;
    let saved_request = api
        .request(
            "real-context-check",
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions/{session_id}/context/requests/{attempt}"),
                body: None,
            },
        )
        .await?;
    assert_eq!(saved_request["status"], "succeeded");
    assert!(
        saved_request["parts"]
            .as_array()
            .ok_or("上下文内容缺失")?
            .iter()
            .any(|part| part["content"]
                .as_str()
                .is_some_and(|content| content.contains(&retained)))
    );
    let trajectory = api
        .request(
            "real-context-check",
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions/{session_id}/trajectory"),
                body: None,
            },
        )
        .await?;
    assert_eq!(trajectory["total_runs"], 2);
    let latest = &trajectory["runs"][0];
    let record = latest["records"]
        .as_array()
        .ok_or("轨迹记录缺失")?
        .iter()
        .find(|record| record["kind"] == "model")
        .ok_or("模型轨迹缺失")?;
    let record_id = record["id"].as_str().ok_or("轨迹标识缺失")?;
    let detail = api
        .request(
            "real-context-check",
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/sessions/{session_id}/trajectory/runs/{followup_id}/records/{record_id}"),
                body: None,
            },
        )
        .await?;
    assert!(serde_json::to_string(&detail["input"])?.contains(&retained));
    assert!(
        detail["usage"]["input_tokens"]
            .as_u64()
            .is_some_and(|tokens| tokens > 0)
    );
    store.close().await;
    Ok(())
}
