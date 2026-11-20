use pixoma_studio::Store;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要独立验收数据目录")?);
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let manifest_path = root.join("generation-verification.json");
    let mut manifest: Value = if manifest_path.exists() {serde_json::from_slice(&std::fs::read(&manifest_path)?)?} else {
        let source = PathBuf::from(std::env::var_os("PIXOMA_TEST_GENERATION_DB").ok_or("需要真实生成配置来源")?);
        let pool = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(source).read_only(true).immutable(true)).await?;
        let (definition,inputs,connection_id): (String,String,String) = sqlx::query_as("SELECT definition,inputs,connection_id FROM online_executions WHERE status='succeeded' AND json_extract(definition,'$.execution.provider')='seedance' AND json_extract(definition,'$.execution.capability')='text_to_video' ORDER BY created_at LIMIT 1").fetch_one(&pool).await?;
        let (provider,base_url,key): (String,String,String) = sqlx::query_as("SELECT provider,base_url,api_key FROM generation_services WHERE id=?").bind(connection_id).fetch_one(&pool).await?;
        pool.close().await;
        let connection = store.save_generation_connection(json!({"name":"视频生成记录验收","provider":provider,"base_url":base_url,"api_key":key})).await?;
        let mut definition: Value = serde_json::from_str(&definition)?;
        definition["execution"]["connection_id"] = connection["id"].clone();
        let workflow = store.create_workflow(json!({"name":"分镜任务真实验收","category":"video","enabled":true,"draft":definition})).await?;
        let workflow_id = workflow["id"].as_str().ok_or("工作流标识缺失")?;
        let version = store.create_workflow_version(workflow_id,json!({"expected_revision":1,"version":"1.0.0"})).await?;
        store.set_default_workflow_version(workflow_id,version["id"].as_str().ok_or("工作流版本缺失")?).await?;
        let session = store.create_session("video-generation-actual-session").await?;
        let inputs: Value = serde_json::from_str(&inputs)?;
        let shot = json!({"id":"tea-opening","name":"茶园晨光","script":"","storyboard":"晨光中的茶叶与露珠","references":[],"subtitle":"","duration":0,"sourceIn":0,"workflowId":workflow_id,"versionId":version["id"],"inputs":inputs});
        let saved = store.write_video_project(&session.id,"initial",json!({"expected_version":0,"project":{"version":2,"name":"分镜生成记录验收","width":1280,"height":720,"fps":30,"shots":[shot],"audio":[]}})).await?;
        json!({"session_id":session.id,"video_id":saved["asset_id"],"workflow_id":workflow_id,"version_id":version["id"],"inputs":inputs,"status":"prepared"})
    };
    std::fs::write(&manifest_path,serde_json::to_vec_pretty(&manifest)?)?;
    let video_id = manifest["video_id"].as_str().ok_or("作品标识缺失")?.to_owned();
    if manifest["legacy_id"].is_null() {
        let execution = store.create_local_execution(json!({"request_id":"video-generation-legacy-actual","workflow_id":manifest["workflow_id"],"version_id":manifest["version_id"],"session_id":manifest["session_id"],"inputs":manifest["inputs"]})).await?;
        let mut work = store.read_video_work(&video_id).await?;
        work["project"]["shots"][0]["executionId"] = execution["id"].clone();
        store.write_video_work(&video_id,manifest["session_id"].as_str().ok_or("对话标识缺失")?,"attach-legacy-execution",json!({"expected_version":work["asset_version"],"project":work["project"]})).await?;
        let records = store.list_video_generations(&video_id).await?;
        let legacy = records.as_array().and_then(|records|records.iter().find(|record|record["execution_id"]==execution["id"])).ok_or("历史任务没有登记")?;
        assert_eq!(legacy["snapshot"]["legacy"],true);
        assert_eq!(legacy["snapshot"]["inputs"],manifest["inputs"]);
        manifest["legacy_id"] = legacy["id"].clone();
        std::fs::write(&manifest_path,serde_json::to_vec_pretty(&manifest)?)?;
    }
    for stage in ["legacy_id","retry_id"] {
        if stage == "retry_id" && manifest[stage].is_null() {
            let retry = store.retry_video_generation(&video_id,manifest["legacy_id"].as_str().ok_or("历史记录缺失")?,json!({"request_id":"video-generation-retry-actual","session_id":manifest["session_id"]})).await?;
            manifest[stage] = retry["id"].clone();
            std::fs::write(&manifest_path,serde_json::to_vec_pretty(&manifest)?)?;
        }
        let id = manifest[stage].as_str().ok_or("生成记录标识缺失")?.to_owned();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
        let mut previous = Value::Null;
        loop {
            let record = store.get_video_generation(&video_id,&id).await?;
            if previous != record["status"] {
                println!("{}",json!({"stage":stage,"id":id,"status":record["status"]}));
                previous = record["status"].clone();
            }
            if record["status"] == "succeeded" {
                let outputs = record["execution"]["outputs"].as_array().ok_or("视频输出缺失")?;
                assert!(outputs.iter().any(|output|output["media_type"].as_str().is_some_and(|mime|mime.starts_with("video/"))));
                for output in outputs {
                    let bytes = store.read_asset(output["asset_id"].as_str().ok_or("视频资产缺失")?,output["asset_version_id"].as_str().ok_or("视频版本缺失")?).await?;
                    assert!(!bytes.is_empty());
                }
                manifest[format!("{stage}_record")] = record;
                std::fs::write(&manifest_path,serde_json::to_vec_pretty(&manifest)?)?;
                break;
            }
            if ["submission_failed","failed","cancelled","submission_unknown","result_unknown"].contains(&record["status"].as_str().ok_or("生成状态缺失")?) {
                return Err(format!("真实视频生成未成功：{} {}",record["submission_error"],record["execution"]["error"]).into());
            }
            if tokio::time::Instant::now() >= deadline {return Err("真实生成仍未返回结果，保留任务编号供查询".into());}
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
    assert_eq!(manifest["legacy_id_record"]["snapshot"],manifest["retry_id_record"]["snapshot"]);
    assert_ne!(manifest["legacy_id_record"]["execution_id"],manifest["retry_id_record"]["execution_id"]);
    assert_eq!(store.list_video_generations(&video_id).await?.as_array().ok_or("生成记录缺失")?.len(),2);
    store.close().await;
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    assert_eq!(store.list_video_generations(&video_id).await?.as_array().ok_or("生成记录缺失")?.len(),2);
    manifest["status"] = json!("verified");
    std::fs::write(&manifest_path,serde_json::to_vec_pretty(&manifest)?)?;
    println!("{}",json!({"success":true,"video_id":video_id,"actual_generations":2,"legacy_imported":true,"retry_snapshot_preserved":true,"reopened":true}));
    store.close().await;
    Ok(())
}
