use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("PIXOMA_STUDIO_DATA_DIR")?);
    std::fs::create_dir_all(&root)?;
    let lock = std::fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".application.lock"))?;
    lock.try_lock()?;
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let model: Value = serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    api.request(
        LocalRequest {
            method: "POST".to_owned(),
            path: "/api/v1/studio-operations/models".to_owned(),
            body: Some(model),
        },
    )
    .await?;
    api.request(
        LocalRequest {
            method: "POST".to_owned(),
            path: "/api/v1/studio-operations/connectors".to_owned(),
            body: Some(json!({
                "name": "安装包 MCP 检查",
                "url": std::env::var("PIXOMA_TEST_MCP_URL")?,
                "credential": std::env::var("PIXOMA_TEST_MCP_TOKEN")?,
                "enabled": true,
                "policy": "approval",
            })),
        },
    )
    .await?;
    store.close().await;
    println!("安装包检查的真实模型和 MCP 配置已保存");
    Ok(())
}
