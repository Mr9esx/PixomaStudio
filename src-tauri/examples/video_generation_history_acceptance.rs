use pixoma_studio::Store;
use serde_json::{Value, json};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要独立验收数据库目录")?);
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let pool = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(root.join("studio-standalone.sqlite")).read_only(true)).await?;
    let works: Vec<String> = sqlx::query_scalar("SELECT id FROM video_works ORDER BY id").fetch_all(&pool).await?;
    pool.close().await;
    let mut verified = Vec::new();
    for video_id in works {
        let original = store.read_video_work(&video_id).await?;
        let records = store.list_video_generations(&video_id).await?;
        let records = records.as_array().ok_or("生成记录缺失")?;
        if records.is_empty() {continue;}
        for record in records {
            let execution = store.get_local_execution(record["execution_id"].as_str().ok_or("原任务标识缺失")?).await?;
            assert_eq!(record["snapshot"]["inputs"],execution["inputs"]);
            assert_eq!(record["execution"]["outputs"],execution["outputs"]);
            assert_eq!(record["snapshot"]["legacy"],true);
            assert!(record["snapshot"]["project_version"].is_null());
        }
        assert_eq!(store.read_video_work(&video_id).await?["project"],original["project"]);
        assert_eq!(store.list_video_generations(&video_id).await?.as_array().ok_or("生成记录缺失")?.len(),records.len());
        let mut edited = original["project"].clone();
        edited["shots"] = json!([]);
        edited["audio"] = json!([]);
        store.write_video_work(&video_id,original["main_session_id"].as_str().ok_or("作品对话缺失")?,"remove-historical-shots",json!({"expected_version":original["asset_version"],"project":edited})).await?;
        assert_eq!(store.list_video_generations(&video_id).await?.as_array().ok_or("生成记录缺失")?.len(),records.len());
        for record in records {
            for output in record["execution"]["outputs"].as_array().ok_or("任务输出缺失")? {
                let bytes = store.read_asset(output["asset_id"].as_str().ok_or("资产缺失")?,output["asset_version_id"].as_str().ok_or("资产版本缺失")?).await?;
                assert!(!bytes.is_empty());
            }
        }
        verified.push(json!({"video_id":video_id,"records":records.len()}));
    }
    if verified.is_empty() {return Err("验收数据库没有包含真实历史任务的作品".into());}
    store.close().await;
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    for entry in &verified {
        let records = store.list_video_generations(entry["video_id"].as_str().ok_or("作品标识缺失")?).await?;
        assert_eq!(records.as_array().ok_or("生成记录缺失")?.len() as u64,entry["records"].as_u64().ok_or("记录数量缺失")?);
    }
    let result: Value = json!({"success":true,"works":verified,"original_inputs_preserved":true,"actual_outputs_readable":true,"deleted_shots_history_preserved":true,"reopened":true});
    std::fs::write(root.join("verification.json"),serde_json::to_vec_pretty(&result)?)?;
    println!("{result}");
    store.close().await;
    Ok(())
}
