use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::{Value, json};

#[tokio::test]
async fn session_and_expert_references_keep_the_selected_local_workflow_version()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("workflow-references-").tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let model = api.request(LocalRequest {
        method: "POST".to_owned(), path: "/api/v1/studio-operations/models".to_owned(),
        body: Some(json!({
            "name": "本地模型", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:1/v1", "model": "local-model",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
        })),
    }).await?;
    let definition = super::workflow_definitions::comfy_definition();
    let workflow = store.create_workflow(json!({ "name": "Klein9B 文生图", "category": "image", "enabled": true, "draft": definition })).await?;
    let id = workflow["id"].as_str().ok_or("工作流标识缺失")?;
    let original = store.create_workflow_version(id, json!({ "expected_revision": 1, "version": "1.0.0" })).await?;
    let mut changed = definition;
    changed["execution"]["workflow"]["60"]["inputs"]["seed"] = json!(43);
    store.update_workflow(id, json!({ "expected_revision": 1, "draft": changed })).await?;
    let latest = store.create_workflow_version(id, json!({ "expected_revision": 2, "version": "1.1.0" })).await?;
    store.set_default_workflow_version(id, latest["id"].as_str().ok_or("最新版本标识缺失")?).await?;
    let part = json!({ "type": "workflow_ref", "workflow_id": id, "version_id": original["id"] });
    let input = json!({ "request_id": "selected-local-version", "text": "使用这个工作流", "permission_mode": "auto_approve", "parts": [part] });
    let queued = store.submit_message(input.clone()).await?;
    assert_eq!(queued["message"]["parts"][0]["version_id"], original["id"]);
    assert_eq!(store.submit_message(input).await?["run"]["id"], queued["run"]["id"]);
    let expert = store.save_expert(None, json!({
        "schema_version": 2, "name": "图片创作", "description": "生成图片", "model_id": model["id"],
        "prompt_parts": [{ "type": "workflow", "workflow_id": id, "version_id": original["id"] }],
        "avatar_asset_id": "", "avatar_version_id": "", "examples": [], "skill_refs": [], "workflow_refs": [], "sop": [], "version": "1.0.0", "enabled": true
    })).await?;
    let expert_input = json!({ "request_id": "expert-local-version", "text": "使用专家绑定的工作流", "permission_mode": "auto_approve", "expert_id": expert["id"], "parts": [part] });
    let expert_run = store.submit_message(expert_input.clone()).await?;
    let mut invalid = expert_input;
    invalid["request_id"] = json!("different-version");
    invalid["parts"][0]["version_id"] = latest["id"].clone();
    assert!(store.submit_message(invalid).await.is_err());
    let run_id = queued["run"]["id"].as_str().ok_or("运行标识缺失")?.to_owned();
    let session_id = expert_run["session"]["id"].as_str().ok_or("会话标识缺失")?.to_owned();
    drop(api);
    store.close().await;
    let reopened = Store::open(&database).await?;
    let detail = reopened.session_detail(&session_id).await?;
    let snapshot = &detail["expert_capabilities"]["workflows"][0];
    assert_eq!(snapshot["version_id"], original["id"]);
    assert_eq!(snapshot["execution"]["workflow"]["60"]["inputs"]["seed"], 42);
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let context: String = sqlx::query_scalar("SELECT context_snapshot FROM runs WHERE id=?").bind(&run_id).fetch_one(&pool).await?;
    let context: Value = serde_json::from_str(&context)?;
    assert_eq!(context["workflows"][0]["version_id"], original["id"]);
    assert_eq!(context["workflows"][0]["execution"]["workflow"]["60"]["inputs"]["seed"], 42);
    pool.close().await;
    reopened.close().await;
    Ok(())
}
