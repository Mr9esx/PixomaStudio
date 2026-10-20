use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::path::PathBuf;

async fn request(
    api: &LocalApi,
    account: &str,
    method: &str,
    path: &str,
    body: Value,
) -> pixoma_studio::Result<Value> {
    api.request(
        account,
        LocalRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            body: Some(body),
        },
    )
    .await
}

fn configuration(name: &str, default: bool) -> Value {
    json!({
        "name": name, "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:11434/v1", "model": "local-model", "api_key": "",
        "enabled": true, "agent_enabled": true, "default": default,
        "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
        "thinking": { "enabled": false },
        "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
    })
}

#[tokio::test]
#[ignore = "需要 PIXOMA_TEST_MCP_URL 与 PIXOMA_TEST_MCP_TOKEN 并连接真实 MCP 服务"]
async fn real_connector_initializes_and_discovers_official_workflow_tools()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("real-mcp-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let connector = request(&api, "real-mcp-check", "POST", "/api/v1/studio-operations/connectors", json!({
        "name": "真实平台 MCP", "url": std::env::var("PIXOMA_TEST_MCP_URL")?,
        "credential": std::env::var("PIXOMA_TEST_MCP_TOKEN")?, "enabled": true, "policy": "approval"
    })).await?;
    let id = connector["id"].as_str().ok_or("MCP 标识缺失")?;
    let checked = request(
        &api,
        "real-mcp-check",
        "POST",
        &format!("/api/v1/studio-operations/connectors/{id}/probe"),
        Value::Null,
    )
    .await?;
    let tools = checked["tools"].as_array().ok_or("工具列表缺失")?;
    assert!(tools.iter().any(|tool| tool["name"] == "run_workflow"));
    assert!(tools.iter().any(|tool| tool["name"] == "get_task"));
    let unauthorized = request(&api, "real-mcp-check", "POST", "/api/v1/studio-operations/connectors/discover", json!({
        "url": std::env::var("PIXOMA_TEST_MCP_URL")?, "credential": uuid::Uuid::new_v4().to_string(),
    })).await.expect_err("错误凭据不能读取真实 MCP 工具");
    assert!(
        matches!(unauthorized, pixoma_studio::Error::Mcp(_)),
        "{unauthorized}"
    );
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let reference: String =
        sqlx::query_scalar("SELECT credential_ref FROM connectors WHERE account_id=? AND id=?")
            .bind("real-mcp-check")
            .bind(id)
            .fetch_one(&connection)
            .await?;
    keyring::Entry::new("com.pixoma.studio", &reference)?.delete_credential()?;
    connection.close().await;
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn model_settings_persist_validate_limits_and_keep_one_default_per_account()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("settings-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let first = request(
        &api,
        "account-a",
        "POST",
        "/api/v1/studio-operations/models",
        configuration("本机模型", true),
    )
    .await?;
    let second = request(
        &api,
        "account-a",
        "POST",
        "/api/v1/studio-operations/models",
        configuration("第二个模型", true),
    )
    .await?;
    let id = first["id"].as_str().ok_or("模型标识缺失")?;
    let path = format!("/api/v1/studio-operations/models/{id}");
    assert!(
        request(
            &api,
            "account-b",
            "PATCH",
            &path,
            configuration("修改", true)
        )
        .await
        .is_err()
    );
    let mut invalid = configuration("无效配置", false);
    invalid["limits"]["max_input_tokens"] = json!(40000);
    assert!(
        request(
            &api,
            "account-a",
            "POST",
            "/api/v1/studio-operations/models",
            invalid
        )
        .await
        .is_err()
    );
    store.close().await;
    let reopened = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(reopened.clone());
    let models = request(
        &api,
        "account-a",
        "GET",
        "/api/v1/studio-operations/models",
        json!(null),
    )
    .await?;
    let models = models.as_array().ok_or("模型列表缺失")?;
    assert_eq!(models.len(), 2);
    assert_eq!(
        models
            .iter()
            .filter(|model| model["default"] == true)
            .count(),
        1
    );
    assert_eq!(
        models
            .iter()
            .find(|model| model["default"] == true)
            .ok_or("默认模型缺失")?["id"],
        second["id"]
    );
    assert!(models.iter().all(|model| model.get("api_key").is_none()));
    assert!(
        request(
            &api,
            "account-b",
            "GET",
            "/api/v1/studio-operations/models",
            json!(null)
        )
        .await?
        .as_array()
        .ok_or("模型列表缺失")?
        .is_empty()
    );
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn model_connection_test_calls_the_configured_endpoint_and_reports_connection_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("settings-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let mut input = configuration("连接检查", false);
    input["base_url"] = json!("http://127.0.0.1:1/v1");
    let model = request(
        &api,
        "account-a",
        "POST",
        "/api/v1/studio-operations/models",
        input,
    )
    .await?;
    let id = model["id"].as_str().ok_or("模型标识缺失")?;
    let error = request(
        &api,
        "account-a",
        "POST",
        &format!("/api/v1/studio-operations/models/{id}/test"),
        json!(null),
    )
    .await
    .expect_err("不可连接的地址不应通过检查");
    assert!(error.to_string().starts_with("模型请求失败："), "{error}");
    assert!(error.to_string().contains("HttpError"), "{error}");
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn connector_settings_are_local_and_probe_reports_actual_connection_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("settings-")
        .tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let input = json!({ "name": "本地 MCP", "url": "http://127.0.0.1:1/mcp", "credential": "", "enabled": true, "policy": "approval" });
    let connector = request(
        &api,
        "account-a",
        "POST",
        "/api/v1/studio-operations/connectors",
        input.clone(),
    )
    .await?;
    let id = connector["id"].as_str().ok_or("MCP 标识缺失")?;
    assert!(connector.get("credential").is_none());
    assert_eq!(connector["tools"], json!(null));
    assert!(
        request(
            &api,
            "account-b",
            "PATCH",
            &format!("/api/v1/studio-operations/connectors/{id}"),
            input.clone()
        )
        .await
        .is_err()
    );
    let mut invalid = input.clone();
    invalid["policy"] = json!("unknown");
    assert!(
        request(
            &api,
            "account-a",
            "POST",
            "/api/v1/studio-operations/connectors",
            invalid
        )
        .await
        .is_err()
    );
    let error = request(
        &api,
        "account-a",
        "POST",
        &format!("/api/v1/studio-operations/connectors/{id}/probe"),
        json!(null),
    )
    .await
    .expect_err("不可连接的服务不应通过检查");
    assert!(error.to_string().starts_with("MCP 连接失败："), "{error}");
    assert_eq!(
        request(
            &api,
            "account-b",
            "GET",
            "/api/v1/studio-operations/connectors",
            json!(null)
        )
        .await?,
        json!([])
    );
    store.close().await;
    let reopened = Store::open(&directory.path().join("studio.sqlite")).await?;
    let connectors = request(
        &LocalApi::new(reopened.clone()),
        "account-a",
        "GET",
        "/api/v1/studio-operations/connectors",
        json!(null),
    )
    .await?;
    assert_eq!(connectors[0]["id"], id);
    assert_eq!(connectors[0]["policy"], "approval");
    reopened.close().await;
    Ok(())
}
