use pixoma_studio::Store;
use serde_json::{Value, json};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要数据目录")?);
    let manifest_path = root.join("h3-manifest.json");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    let execution_id = manifest["execution_id"].as_str().ok_or("执行标识缺失")?.to_owned();
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let execution = store.get_local_execution(&execution_id).await?;
    let outputs = execution["outputs"].as_array().ok_or("输出缺失")?;
    for output in outputs.iter().filter(|output| output["media_type"].as_str().is_some_and(|value| value.starts_with("video/"))) {
        let frames = store.ensure_video_boundary_outputs(
            execution["session_id"].as_str(), &execution_id,
            output["asset_id"].as_str().ok_or("视频资产缺失")?,
            output["asset_version_id"].as_str().ok_or("视频版本缺失")?,
            output["index"].as_i64().ok_or("输出索引缺失")?,
        ).await?;
        println!("{}", json!({"execution_id":execution_id,"frames":frames}));
    }
    manifest["outputs"] = store.get_local_execution(&execution_id).await?["outputs"].clone();
    std::fs::write(manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    if let Some(session) = execution["session_id"].as_str() {
        for asset in store.list_session_assets(session).await?.iter().filter(|asset|asset.kind == "video") {
            for version in &asset.versions {
                let metadata = store.ensure_video_metadata(&asset.id,&version.id).await?;
                println!("{}",json!({"asset_id":asset.id,"version_id":version.id,"duration":metadata["duration"],"fps":metadata["fps"]}));
            }
        }
    }
    Ok(())
}
