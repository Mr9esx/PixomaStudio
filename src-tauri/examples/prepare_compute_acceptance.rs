use pixoma_studio::{Store, Result};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    let root = std::path::PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").expect("需要验收数据目录"));
    let address = std::env::var("PIXOMA_TEST_COMFY_URL").expect("需要真实 ComfyUI 地址");
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let credential = store.create_compute_node(json!({"name":"安装包节点","location":"local","enabled":true,"comfyui_base_url":address,"capabilities":["图片"]})).await?;
    println!("{}", json!({"node_id":credential["node"]["id"],"prepared":true}));
    store.close().await;
    Ok(())
}
