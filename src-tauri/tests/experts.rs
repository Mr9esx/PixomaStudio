use pixoma_studio::{AgentRuntime, LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::path::PathBuf;

async fn execute_expert(
    store: &Store,
    run: &str,
    database: &std::path::Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let outcome = AgentRuntime::new(store.clone())
        .execute(run)
        .await?;
    if outcome["status"] != "succeeded" {
        let connection =
            sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
        let calls: Vec<String> = sqlx::query_scalar("SELECT json_object('call',json(call),'status',status,'result',json(result)) FROM tool_calls WHERE run_id=? ORDER BY turn,part")
            .bind(run).fetch_all(&connection).await?;
        connection.close().await;
        eprintln!("专家运行的实际工具记录：{}", serde_json::to_string(&calls)?);
    }
    Ok(outcome)
}

#[tokio::test]
async fn expert_session_lists_the_skill_version_bound_to_its_prompt()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("expert-capabilities-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let model = api.request(LocalRequest {
        method: "POST".to_owned(), path: "/api/v1/studio-operations/models".to_owned(),
        body: Some(json!({
            "name": "本机模型", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:11434/v1", "model": "local-model", "api_key": "",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false },
            "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true },
        })),
    }).await?;
    let skill = api.request(LocalRequest {
        method: "POST".to_owned(), path: "/api/v1/studio/skills".to_owned(),
        body: Some(json!({ "name": "文档检查", "description": "检查正文编号", "prompt": "检查正文中的编号。", "version": "1.0.0", "enabled": true })),
    }).await?;
    let image = include_bytes!("../../public/images/pixoma-ghost.png");
    let avatar = store.upload_asset(json!({ "file_name": "头像.png", "project_id": "", "request_id": "avatar", "purpose": "expert_avatar" }), image).await?;
    let expert = store.save_expert(None, json!({
        "schema_version": 2, "name": "内容检查", "description": "使用指定技能检查内容", "model_id": model["id"],
        "prompt_parts": [{ "type": "skill", "skill_id": skill["id"], "version": "1.0.0" }],
        "avatar_asset_id": avatar["id"], "avatar_version_id": avatar["versions"][0]["id"], "examples": [], "skill_refs": [], "workflow_refs": [],
        "sop": [], "version": "1.0.0", "enabled": true,
    })).await?;
    let queued = store.submit_message(json!({ "request_id": "bound-capabilities", "text": "检查内容", "permission_mode": "auto_approve", "expert_id": expert["id"] })).await?;
    let mut changed = expert.clone();
    changed["version"] = json!("1.1.0");
    changed["avatar_asset_id"] = json!("");
    changed["avatar_version_id"] = json!("");
    store
        .save_expert(expert["id"].as_str(), changed)
        .await?;
    api.request(LocalRequest {
        method: "PATCH".to_owned(), path: format!("/api/v1/studio/skills/{}", skill["id"].as_str().ok_or("技能标识缺失")?),
        body: Some(json!({ "name": "更新的文档检查", "description": "检查更新后的正文", "prompt": "检查更新后的正文。", "version": "1.1.0", "enabled": false, "updated_at": skill["updated_at"] })),
    }).await?;
    drop(api);
    store.close().await;
    let store = Store::open(&database).await?;
    let detail = store
        .session_detail(
            queued["session"]["id"].as_str().ok_or("会话标识缺失")?,
        )
        .await?;
    let expert_id = expert["id"].as_str().ok_or("专家标识缺失")?;
    let media = store
        .expert_avatar_response(expert_id, "1.0.0", "GET")
        .await?;
    assert_eq!(media.mime_type, "image/png");
    assert_eq!(media.body, image);
    let head = store
        .expert_avatar_response(expert_id, "1.0.0", "HEAD")
        .await?;
    assert!(head.body.is_empty());
    assert_eq!(head.content_length, image.len() as u64);
    assert!(
        store
            .expert_avatar_response(expert_id, "1.1.0", "GET")
            .await
            .is_err()
    );
    assert!(
        store
            .expert_avatar_response("missing-expert", "1.0.0", "GET")
            .await
            .is_err()
    );
    assert_eq!(
        detail["expert_capabilities"]["skills"]
            .as_array()
            .ok_or("专家技能列表缺失")?
            .len(),
        1
    );
    assert_eq!(
        detail["expert_capabilities"]["skills"][0]["id"],
        skill["id"]
    );
    assert_eq!(
        detail["expert_capabilities"]["skills"][0]["version"],
        "1.0.0"
    );
    assert_eq!(
        detail["expert_capabilities"]["skills"][0]["name"],
        "文档检查"
    );
    assert_eq!(
        detail["expert_capabilities"]["skills"][0]["description"],
        "检查正文编号"
    );
    let api = LocalApi::new(store.clone());
    let current = api
        .request(
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/experts/{expert_id}"),
                body: None,
            },
        )
        .await?;
    assert_eq!(current["enabled"], true);
    assert_eq!(
        current["available"], false,
        "已停用的引用技能应影响专家可用状态"
    );
    assert!(
        !current["unavailable_reason"]
            .as_str()
            .ok_or("专家不可用原因缺失")?
            .is_empty()
    );
    let list = api
        .request(
            LocalRequest {
                method: "GET".to_owned(),
                path: "/api/v1/studio/experts".to_owned(),
                body: None,
            },
        )
        .await?;
    assert_eq!(list[0]["available"], current["available"]);
    assert_eq!(list[0]["unavailable_reason"], current["unavailable_reason"]);
    assert!(store.submit_message(json!({ "session_id": queued["session"]["id"], "request_id": "disabled-skill", "text": "继续检查", "permission_mode": "auto_approve" })).await.is_err());
    api.request(
        LocalRequest {
            method: "PATCH".to_owned(),
            path: format!(
                "/api/v1/studio/skills/{}/enabled",
                skill["id"].as_str().ok_or("技能标识缺失")?
            ),
            body: Some(json!({ "enabled": true })),
        },
    )
    .await?;
    assert_eq!(
        api.request(
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/experts/{expert_id}"),
                body: None
            }
        )
        .await?["available"],
        true
    );
    let mut configuration = model.as_object().ok_or("模型配置格式无效")?.clone();
    for field in ["id", "api_key_masked", "has_api_key"] {
        configuration.remove(field);
    }
    configuration["capabilities"]["tools"] = json!(false);
    let model_path = format!(
        "/api/v1/studio-operations/models/{}",
        model["id"].as_str().ok_or("模型标识缺失")?
    );
    api.request(
        LocalRequest {
            method: "PATCH".to_owned(),
            path: model_path.clone(),
            body: Some(Value::Object(configuration.clone())),
        },
    )
    .await?;
    assert_eq!(
        api.request(
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/experts/{expert_id}"),
                body: None
            }
        )
        .await?["available"],
        false
    );
    configuration["capabilities"]["tools"] = json!(true);
    api.request(
        LocalRequest {
            method: "PATCH".to_owned(),
            path: model_path.clone(),
            body: Some(Value::Object(configuration.clone())),
        },
    )
    .await?;
    let mut second_configuration = configuration.clone();
    second_configuration["name"] = json!("另一项本地模型");
    second_configuration["default"] = json!(false);
    let second_model = api
        .request(
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(Value::Object(second_configuration.clone())),
            },
        )
        .await?;
    let mut latest = api
        .request(
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/experts/{expert_id}"),
                body: None,
            },
        )
        .await?;
    latest["version"] = json!("1.2.0");
    latest["model_id"] = second_model["id"].clone();
    store.save_expert(Some(expert_id), latest).await?;
    second_configuration["enabled"] = json!(false);
    api.request(
        LocalRequest {
            method: "PATCH".to_owned(),
            path: format!(
                "/api/v1/studio-operations/models/{}",
                second_model["id"].as_str().ok_or("第二项模型标识缺失")?
            ),
            body: Some(Value::Object(second_configuration)),
        },
    )
    .await?;
    assert_eq!(
        api.request(
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/experts/{expert_id}"),
                body: None
            }
        )
        .await?["available"],
        false
    );
    let continued = store.submit_message(json!({ "session_id": queued["session"]["id"], "request_id": "original-bound-expert", "text": "继续使用当前会话的专家版本", "permission_mode": "auto_approve" })).await?;
    assert_eq!(continued["run"]["expert_version"], "1.0.0");
    assert_eq!(continued["run"]["model_config_id"], model["id"]);
    configuration["enabled"] = json!(false);
    configuration["default"] = json!(false);
    api.request(
        LocalRequest {
            method: "PATCH".to_owned(),
            path: model_path,
            body: Some(Value::Object(configuration)),
        },
    )
    .await?;
    assert_eq!(
        api.request(
            LocalRequest {
                method: "GET".to_owned(),
                path: format!("/api/v1/studio/experts/{expert_id}"),
                body: None
            }
        )
        .await?["available"],
        false
    );
    assert!(
        store
            .session_detail("missing-session")
            .await
            .is_err()
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接真实模型服务"]
async fn real_expert_cards_keep_user_decisions_and_actual_document_results_after_reopening()
-> Result<(), Box<dyn std::error::Error>> {
    let configuration: Value = serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("expert-cards-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let mut store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let model = api
        .request(
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration),
            },
        )
        .await?;
    let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
    let expert = store.save_expert(None, json!({
        "schema_version": 2, "name": "文档保存", "description": "确认正文并保存文档", "model_id": model_id,
        "prompt_parts": [{ "type": "text", "text": "严格通过专家工具管理任务。准备文档时先 update_expert_task create_task，再用 request_expert_confirmation 创建 write 步骤的 step_entry 卡片，fields 只有必填 textarea 字段 content。等待用户决定，不能提前创建文档。用户 start 后，使用已记录的输入 content 调用 create_text_asset，文档名为已确认文档，正文保持用户输入原样。采用工具返回的真实资产和版本，用 request_expert_confirmation 创建 result_review 卡片。用户 revise 后，根据其最新要求使用 update_text_asset 更新原资产，保存新版本并展示新版本的 result_review 卡片。用户 accept 后保存完成状态。每次模型输出只调用一个专家控制工具。" }],
        "avatar_asset_id": "", "avatar_version_id": "", "examples": [], "skill_refs": [], "workflow_refs": [],
        "sop": [{ "id": "write", "title": "保存文档", "instruction_parts": [{ "type": "text", "text": "使用步骤输入卡片取得正文和开始决定，实际保存文档后展示步骤结果卡片。" }] }],
        "version": "1.0.0", "enabled": true,
    })).await?;
    let queued = store.submit_message(json!({
        "request_id": "prepare-document", "text": "请准备文档，展示正文输入表单，等待我确认。", "permission_mode": "auto_approve",
        "expert_id": expert["id"], "expert_version": "1.0.0",
    })).await?;
    let session = queued["session"]["id"].as_str().ok_or("会话标识缺失")?;
    let run = queued["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let outcome = execute_expert(&store, run, &database).await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let detail = store.session_detail(session).await?;
    assert!(
        detail["assets"]
            .as_array()
            .ok_or("资产列表缺失")?
            .is_empty()
    );
    assert_eq!(
        detail["expert_cards"]
            .as_array()
            .ok_or("专家卡片缺失")?
            .len(),
        1
    );
    assert_eq!(
        detail["expert_cards"][0]["expert_card"]["mode"],
        "step_entry"
    );
    let card = detail["expert_cards"][0]["id"]
        .as_str()
        .ok_or("专家卡片标识缺失")?;
    drop(api);
    store.close().await;
    store = Store::open(&database).await?;
    assert_eq!(store.recover_interrupted_runs().await?, 0);
    let response_id = uuid::Uuid::new_v4().to_string();
    assert!(store.resolve_expert_card_response("missing-card", json!({ "response_id": response_id, "action": "start", "inputs": { "content": "文档" } })).await.is_err());
    assert!(
        store
            .resolve_expert_card_response(
                card,
                json!({ "response_id": response_id, "action": "start", "inputs": {} })
            )
            .await
            .is_err()
    );
    let content = format!("已经确认的正文：{}", uuid::Uuid::new_v4());
    let response = json!({ "response_id": response_id, "action": "start", "inputs": { "content": content }, "comment": "请保留正文原样。" });
    let (first, second) = tokio::join!(
        store.resolve_expert_card_response(card, response.clone()),
        store.resolve_expert_card_response(card, response.clone()),
    );
    let started = first?;
    assert_eq!(second?["id"], started["id"]);
    assert_eq!(
        store
            .resolve_expert_card_response(card, response)
            .await?["id"],
        started["id"]
    );
    assert!(
        store
            .resolve_expert_card_response(
                card,
                json!({ "response_id": response_id, "action": "skip" })
            )
            .await
            .is_err()
    );
    let outcome = execute_expert(
        &store,
        started["id"].as_str().ok_or("卡片响应运行缺失")?,
        &database,
    )
    .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let detail = store.session_detail(session).await?;
    let assets = store.list_session_assets(session).await?;
    assert_eq!(assets.len(), 1);
    assert_eq!(
        store
            .read_asset(&assets[0].id, &assets[0].versions[0].id)
            .await?,
        content.as_bytes()
    );
    let review = detail["expert_cards"]
        .as_array()
        .ok_or("专家卡片缺失")?
        .iter()
        .find(|card| card["status"] == "pending" && card["expert_card"]["mode"] == "result_review")
        .ok_or("结果卡片缺失")?;
    assert_eq!(
        review["expert_card"]["results"]["assets"][0]["asset_id"],
        assets[0].id
    );
    assert_eq!(
        review["expert_card"]["results"]["assets"][0]["asset_version_id"],
        assets[0].versions[0].id
    );
    let review_id = review["id"].as_str().ok_or("结果卡片标识缺失")?;
    let revised_content = format!("修改后的正文：{}", uuid::Uuid::new_v4());
    let revised = store.resolve_expert_card_response(review_id, json!({ "response_id": uuid::Uuid::new_v4().to_string(), "action": "revise", "comment": format!("请把文档正文完整修改为：{revised_content}") })).await?;
    let outcome = execute_expert(
        &store,
        revised["id"].as_str().ok_or("结果修改运行缺失")?,
        &database,
    )
    .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let assets = store.list_session_assets(session).await?;
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].versions.len(), 2);
    assert_eq!(
        store
            .read_asset(&assets[0].id, &assets[0].versions[0].id)
            .await?,
        content.as_bytes()
    );
    assert_eq!(
        store
            .read_asset(&assets[0].id, &assets[0].versions[1].id)
            .await?,
        revised_content.as_bytes()
    );
    let detail = store.session_detail(session).await?;
    let review = detail["expert_cards"]
        .as_array()
        .ok_or("专家卡片缺失")?
        .iter()
        .find(|card| card["status"] == "pending" && card["expert_card"]["mode"] == "result_review")
        .ok_or("修改后的结果卡片缺失")?;
    assert!(
        review["expert_card"]["results"]["assets"]
            .as_array()
            .ok_or("结果资产缺失")?
            .iter()
            .any(|reference| reference["asset_id"] == assets[0].id
                && reference["asset_version_id"] == assets[0].versions[1].id)
    );
    let review_id = review["id"].as_str().ok_or("结果卡片标识缺失")?;
    let accepted = store
        .resolve_expert_card_response(
            review_id,
            json!({ "response_id": uuid::Uuid::new_v4().to_string(), "action": "accept" }),
        )
        .await?;
    let outcome = execute_expert(
        &store,
        accepted["id"].as_str().ok_or("结果确认运行缺失")?,
        &database,
    )
    .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let detail = store.session_detail(session).await?;
    assert_eq!(detail["expert_tasks"][0]["steps"][0]["status"], "completed");
    assert!(
        detail["expert_cards"]
            .as_array()
            .ok_or("专家卡片缺失")?
            .iter()
            .all(|card| card["status"] != "pending")
    );
    assert_eq!(store.list_session_assets(session).await?.len(), 1);
    let queued = store.submit_message(json!({
        "request_id": "deferred-document", "text": "请准备另一份文档，展示正文输入表单，等待我确认。", "permission_mode": "auto_approve",
        "expert_id": expert["id"], "expert_version": "1.0.0",
    })).await?;
    let other_session = queued["session"]["id"].as_str().ok_or("会话标识缺失")?;
    let outcome = execute_expert(
        &store,
        queued["run"]["id"].as_str().ok_or("运行标识缺失")?,
        &database,
    )
    .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let detail = store.session_detail(other_session).await?;
    let entry = detail["expert_cards"]
        .as_array()
        .ok_or("专家卡片缺失")?
        .iter()
        .find(|card| card["status"] == "pending")
        .ok_or("步骤输入卡片缺失")?;
    let deferred = store.resolve_expert_card_response(entry["id"].as_str().ok_or("卡片标识缺失")?, json!({
        "response_id": uuid::Uuid::new_v4().to_string(), "action": "defer", "comment": "资料尚未准备完整，暂时不要执行步骤或创建文档，请等待我继续补充。",
    })).await?;
    let outcome = execute_expert(
        &store,
        deferred["id"].as_str().ok_or("补充资料运行缺失")?,
        &database,
    )
    .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    assert!(store.list_session_assets(other_session).await?.is_empty());
    store.close().await;
    store = Store::open(&database).await?;
    assert_eq!(store.recover_interrupted_runs().await?, 0);
    let mut superseded = None;
    let mut current = None;
    for request_id in ["show-entry", "refresh-entry"] {
        let queued = store.submit_message(json!({
            "session_id": other_session, "request_id": request_id, "text": "请重新展示最新的正文输入卡片，继续等待我的开始决定。", "permission_mode": "auto_approve",
        })).await?;
        let outcome = execute_expert(
            &store,
            queued["run"]["id"].as_str().ok_or("显示卡片运行缺失")?,
            &database,
        )
        .await?;
        assert_eq!(
            outcome["status"], "succeeded",
            "{}",
            outcome["error_message"]
        );
        let detail = store.session_detail(other_session).await?;
        let revision = &detail["expert_tasks"][0]["revision"];
        let card = detail["expert_cards"]
            .as_array()
            .ok_or("专家卡片缺失")?
            .iter()
            .find(|card| card["status"] == "pending" && card["expected_task_revision"] == *revision)
            .ok_or("当前输入卡片缺失")?;
        superseded = current;
        current = Some(card["id"].as_str().ok_or("卡片标识缺失")?.to_owned());
    }
    assert!(store.resolve_expert_card_response(superseded.as_deref().ok_or("旧卡片缺失")?, json!({
        "response_id": uuid::Uuid::new_v4().to_string(), "action": "start", "inputs": { "content": "旧卡片的正文" },
    })).await.is_err());
    let skipped = store.resolve_expert_card_response(current.as_deref().ok_or("当前卡片缺失")?, json!({
        "response_id": uuid::Uuid::new_v4().to_string(), "action": "skip", "comment": "跳过保存文档。",
    })).await?;
    let outcome = execute_expert(
        &store,
        skipped["id"].as_str().ok_or("跳过步骤运行缺失")?,
        &database,
    )
    .await?;
    assert_eq!(
        outcome["status"], "succeeded",
        "{}",
        outcome["error_message"]
    );
    let detail = store.session_detail(other_session).await?;
    assert_eq!(detail["expert_tasks"][0]["steps"][0]["status"], "skipped");
    assert!(store.list_session_assets(other_session).await?.is_empty());
    store.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MODEL_CONFIG 并连接真实模型服务"]
async fn real_model_keeps_the_expert_version_and_local_model_bound_to_the_session()
-> Result<(), Box<dyn std::error::Error>> {
    let configuration: Value = serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("expert-binding-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let mut store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let model = api
        .request(
            LocalRequest {
                method: "POST".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: Some(configuration),
            },
        )
        .await?;
    let model_id = model["id"].as_str().ok_or("模型标识缺失")?;
    let expected = uuid::Uuid::new_v4().to_string();
    let replacement = uuid::Uuid::new_v4().to_string();
    let mut expert = json!({
        "schema_version": 2, "name": "文档检查", "description": "检查文档编号", "model_id": model_id,
        "prompt_parts": [{ "type": "text", "text": format!("本专家的识别编号为 {expected}。用户询问识别编号时，仅回复该完整编号。") }],
        "avatar_asset_id": "", "avatar_version_id": "", "examples": [], "skill_refs": [], "workflow_refs": [],
        "sop": [{ "id": "review", "title": "检查文档", "instruction_parts": [{ "type": "text", "text": "读取文档并核对编号。" }] }],
        "version": "1.0.0", "enabled": true,
    });
    let mut invalid_notes = expert.clone();
    invalid_notes["notes"] = json!(3);
    assert!(
        store
            .save_expert(None, invalid_notes)
            .await
            .is_err()
    );
    let created = store.save_expert(None, expert.clone()).await?;
    let id = created["id"].as_str().ok_or("专家标识缺失")?;
    assert!(
        api.request(
            LocalRequest {
                method: "GET".to_owned(),
                path: "/api/v1/studio/experts/missing-expert".to_owned(),
                body: None
            }
        )
        .await
        .is_err()
    );
    expert["version"] = json!("1.1.0");
    expert["updated_at"] = created["updated_at"].clone();
    expert["prompt_parts"][0]["text"] = json!(format!("本专家的识别编号为 {replacement}。"));
    store.save_expert(Some(id), expert.clone()).await?;
    assert!(store.save_expert(Some(id), expert).await.is_err());
    let queued = store.submit_message(json!({
        "request_id": "expert-binding", "text": "当前只回答专家的识别编号，不需要开始文档检查步骤。",
        "permission_mode": "auto_approve", "expert_id": id, "expert_version": "1.0.0",
    })).await?;
    let run_id = queued["run"]["id"].as_str().ok_or("运行标识缺失")?;
    let session_id = queued["session"]["id"].as_str().ok_or("会话标识缺失")?;
    assert_eq!(queued["session"]["expert_id"], id);
    assert_eq!(queued["session"]["expert_version"], "1.0.0");
    assert_eq!(queued["session"]["model_config_id"], model_id);
    drop(api);
    store.close().await;
    store = Store::open(&database).await?;
    let result = AgentRuntime::new(store.clone())
        .execute(run_id)
        .await?;
    assert_eq!(result["status"], "succeeded", "{}", result["error_message"]);
    let events = store.run_events_after(run_id, 0).await?;
    let output: String = events
        .iter()
        .filter(|event| event["type"] == "TEXT_MESSAGE_CONTENT")
        .filter_map(|event| event["payload"]["delta"].as_str())
        .collect();
    assert!(output.contains(&expected), "模型没有使用会话绑定的专家版本");
    assert!(!output.contains(&replacement));
    let continued = store.submit_message(json!({
        "session_id": session_id, "request_id": "expert-continued", "text": "继续使用本会话专家。", "permission_mode": "auto_approve",
    })).await?;
    assert_eq!(continued["run"]["expert_version"], "1.0.0");
    assert!(store.submit_message(json!({
        "session_id": session_id, "request_id": "expert-changed", "text": "继续检查", "permission_mode": "auto_approve",
        "expert_id": id, "expert_version": "1.1.0",
    })).await.is_err());
    let mut invalid = created.clone();
    invalid["version"] = json!("2.0.0");
    let repeated_step = invalid["sop"][0].clone();
    invalid["sop"]
        .as_array_mut()
        .ok_or("步骤列表缺失")?
        .push(repeated_step);
    assert!(store.save_expert(None, invalid).await.is_err());
    store.close().await;
    Ok(())
}
