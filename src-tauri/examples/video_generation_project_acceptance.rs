use pixoma_studio::Store;
use serde_json::{Value, json};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要独立验收数据库目录")?);
    let path = root.join("studio-standalone.sqlite");
    let store = Store::open(&path).await?;
    let pool = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&path)).await?;
    let works: Vec<String> = sqlx::query_scalar("SELECT id FROM video_works ORDER BY id").fetch_all(&pool).await?;
    if works.is_empty() {return Err("需要已有真实生成结果的验收作品".into());}
    let project = store.create_project("任务结果归属验收").await?;
    for video_id in &works {
        sqlx::query("UPDATE video_works SET project_id=? WHERE id=?").bind(&project.id).bind(video_id).execute(&pool).await?;
    }
    store.clear_sessions().await?;
    let mut verified = Vec::new();
    for video_id in &works {
        let records = store.list_video_generations(video_id).await?;
        let records = records.as_array().ok_or("任务记录缺失")?;
        if records.is_empty() {return Err("验收作品没有真实任务".into());}
        for record in records {
            assert_eq!(record["status"],"succeeded");
            assert!(record["session_id"].is_null());
            let outputs = record["execution"]["outputs"].as_array().ok_or("输出缺失")?;
            assert!(!outputs.is_empty());
            for output in outputs {
                let asset_id = output["asset_id"].as_str().ok_or("资产标识缺失")?;
                let version_id = output["asset_version_id"].as_str().ok_or("资产版本缺失")?;
                assert!(!store.read_asset(asset_id,version_id).await?.is_empty());
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM project_assets WHERE project_id=? AND asset_id=? AND asset_version_id=?")
                    .bind(&project.id).bind(asset_id).bind(version_id).fetch_one(&pool).await?;
                assert_eq!(count,1);
            }
        }
        let opened = store.open_video_work(video_id).await?;
        let session_id = opened["main_session_id"].as_str().ok_or("作品对话没有恢复")?;
        assert_eq!(store.read_video_project(session_id).await?["asset_id"],*video_id);
        verified.push(json!({"video_id":video_id,"records":records.len(),"restored_session_id":session_id}));
    }
    pool.close().await;
    store.close().await;
    let store = Store::open(&path).await?;
    for work in &verified {assert_eq!(store.list_video_generations(work["video_id"].as_str().ok_or("作品标识缺失")?).await?.as_array().ok_or("记录缺失")?.len() as u64,work["records"].as_u64().ok_or("记录数量缺失")?);}
    let result: Value = json!({"success":true,"project_id":project.id,"works":verified,"cleared_chats_preserve_outputs":true,"project_outputs_readable":true,"main_chat_restored":true,"reopened":true});
    std::fs::write(root.join("project-verification.json"),serde_json::to_vec_pretty(&result)?)?;
    println!("{result}");
    store.close().await;
    Ok(())
}
