use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::json;
use std::path::PathBuf;

#[tokio::test]
async fn credentials_persist_in_the_application_database_and_remain_private_to_its_settings()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("database-credentials-")
        .tempdir_in(root)?;
    let database = directory.path().join("studio.sqlite");
    let store = Store::open(&database).await?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('model_configs')")
            .fetch_all(&connection)
            .await?;
    assert!(!columns.iter().any(|column| column == "account_id"));
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('connectors')")
            .fetch_all(&connection)
            .await?;
    assert!(!columns.iter().any(|column| column == "account_id"));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM model_configs")
        .fetch_one(&connection)
        .await?;
    assert_eq!(count, 0);
    let api = LocalApi::new(store.clone());
    let secret = uuid::Uuid::new_v4().to_string();
    let model = api.request(LocalRequest {
        method: "POST".to_owned(),
        path: "/api/v1/studio-operations/models".to_owned(),
        body: Some(json!({
            "name": "数据库模型", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:1/v1", "model": "local-model", "api_key": secret,
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
        })),
    }).await?;
    assert_eq!(model["has_api_key"], true);
    assert!(!model.to_string().contains(&secret));
    let stored: String = sqlx::query_scalar("SELECT api_key FROM model_configs WHERE id=?")
        .bind(model["id"].as_str().ok_or("模型标识缺失")?)
        .fetch_one(&connection)
        .await?;
    assert_eq!(stored, secret);
    let token = uuid::Uuid::new_v4().to_string();
    let connector = api.request(LocalRequest {
        method: "POST".to_owned(),
        path: "/api/v1/studio-operations/connectors".to_owned(),
        body: Some(json!({ "name": "数据库 MCP", "url": "http://127.0.0.1:1/mcp", "credential": token, "enabled": true, "policy": "approval" })),
    }).await?;
    assert!(!connector.to_string().contains(&token));
    let stored: String = sqlx::query_scalar("SELECT credential FROM connectors WHERE id=?")
        .bind(connector["id"].as_str().ok_or("MCP 标识缺失")?)
        .fetch_one(&connection)
        .await?;
    assert_eq!(stored, token);
    drop(api);
    connection.close().await;
    store.close().await;

    let reopened = Store::open(&database).await?;
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}", database.display())).await?;
    let values: Vec<String> = sqlx::query_scalar(
        "SELECT api_key FROM model_configs UNION ALL SELECT credential FROM connectors",
    )
    .fetch_all(&connection)
    .await?;
    assert_eq!(values.len(), 2);
    assert!(values.contains(&secret));
    assert!(values.contains(&token));
    let response = LocalApi::new(reopened.clone())
        .request(
            LocalRequest {
                method: "GET".to_owned(),
                path: "/api/v1/studio-operations/models".to_owned(),
                body: None,
            },
        )
        .await?;
    assert_eq!(response.as_array().ok_or("模型列表缺失")?.len(), 1);
    assert!(!response.to_string().contains(&secret));
    let isolated_path = directory.path().join("isolated/studio.sqlite");
    let isolated = Store::open(&isolated_path).await?;
    let isolated_connection =
        sqlx::SqlitePool::connect(&format!("sqlite://{}", isolated_path.display())).await?;
    let count: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM model_configs)+(SELECT count(*) FROM connectors)")
        .fetch_one(&isolated_connection).await?;
    assert_eq!(count, 0);
    isolated_connection.close().await;
    isolated.close().await;
    connection.close().await;
    reopened.close().await;
    Ok(())
}
