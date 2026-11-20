use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::Value;
use sqlx::{Connection, Row, sqlite::SqliteConnectOptions};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = PathBuf::from(std::env::var_os("PIXOMA_EDITOR_SOURCE").ok_or("需要模型配置来源")?);
    let target = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要验收数据目录")?);
    let mut database = sqlx::SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(source.join("studio-standalone.sqlite")).read_only(true)).await?;
    let row = sqlx::query("SELECT document,api_key FROM model_configs WHERE json_extract(document,'$.default')=1 AND json_extract(document,'$.enabled')=1").fetch_one(&mut database).await?;
    let mut model: Value = serde_json::from_str(row.get("document"))?;
    model["api_key"] = Value::String(row.get("api_key"));
    let store = Store::open(&target.join("studio-standalone.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let saved = api.request(LocalRequest { method: "POST".into(), path: "/api/v1/studio-operations/models".into(), body: Some(model) }).await?;
    println!("{}",serde_json::json!({"model_id":saved["id"],"name":saved["name"],"prepared":true}));
    store.close().await;
    database.close().await?;
    Ok(())
}
