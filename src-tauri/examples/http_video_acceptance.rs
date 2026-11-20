use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use sqlx::Row;
use std::{path::PathBuf, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache/real-video-20261031");
    let database = root.join("studio-standalone.sqlite");
    let store = Store::open(&database).await?;
    let api = LocalApi::new(store.clone());
    let original: Value = serde_json::from_slice(&std::fs::read(root.join("video-acceptance.json"))?)?;
    let path = root.join("http-video-acceptance.json");
    let mut manifest: Value = if path.exists() {serde_json::from_slice(&std::fs::read(&path)?)?} else {
        let pool = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&database)).await?;
        let row = sqlx::query("SELECT api_key FROM generation_services WHERE provider='seedance' ORDER BY created_at LIMIT 1").fetch_one(&pool).await?;
        let connection = store.save_generation_connection(json!({"name":"HTTP JSON · Ark 真实验收","provider":"http_json","base_url":"https://ark.cn-beijing.volces.com","api_key":row.get::<String,_>("api_key")})).await?;
        pool.close().await;
        assert!(connection.get("api_key").is_none());
        let templates = api.request(LocalRequest {method:"GET".into(),path:"/api/v1/studio/generation-templates".into(),body:None}).await?;
        let mut definition = templates[0]["definition"].clone();
        definition["execution"]["provider"] = json!("http_json");
        definition["execution"]["connection_id"] = connection["id"].clone();
        definition["execution"]["http"]["body"]["content"] = json!([{"type":"text","text":""}]);
        definition["execution"]["input_mapping"] = json!({"prompt":"/content/0/text","resolution":"/resolution","ratio":"/ratio","duration":"/duration","generate_audio":"/generate_audio"});
        definition["execution"]["http"]["fixed_mapping"] = definition["execution"]["input_mapping"].clone();
        // duration 使用固定参数，验证开放参数与固定参数共同生成请求。
        definition["inputs"].as_array_mut().ok_or("输入字段无效")?.retain(|field|field["key"]!="duration");
        definition["input_schema"]["properties"].as_object_mut().ok_or("输入 Schema 无效")?.remove("duration");
        definition["input_schema"]["required"].as_array_mut().ok_or("必填字段无效")?.retain(|key|key!="duration");
        definition["execution"]["input_mapping"].as_object_mut().ok_or("输入映射无效")?.remove("duration");
        let workflow = store.create_workflow(json!({"name":"HTTP JSON · Seedance 正式接口验收","category":"video","tags":["真实验收"],"enabled":true,"draft":definition})).await?;
        let workflow_id = workflow["id"].as_str().ok_or("工作流编号缺失")?;
        let version = store.create_workflow_version(workflow_id,json!({"expected_revision":1,"version":"1.0.0","notes":"通用 HTTP JSON 执行验收"})).await?;
        store.set_default_workflow_version(workflow_id,version["id"].as_str().ok_or("版本编号缺失")?).await?;
        json!({"session_id":original["session_id"],"workflow_id":workflow_id,"workflow_version_id":version["id"]})
    };
    std::fs::write(&path,serde_json::to_vec_pretty(&manifest)?)?;
    let request = json!({"request_id":"tea-http-json-acceptance-20261031-v1","workflow_id":manifest["workflow_id"],"version_id":manifest["workflow_version_id"],"session_id":manifest["session_id"],"inputs":{"prompt":"茶品牌短片，微距镜头拍摄清晨茶叶上的透明露珠，镜头轻缓横移，柔和自然光，电影摄影，只有自然环境声音，无文字无标志。","resolution":"720p","ratio":"16:9","generate_audio":true}});
    let mut execution = store.create_local_execution(request.clone()).await?;
    let repeated = store.create_local_execution(request.clone()).await?;
    assert_eq!(execution["id"],repeated["id"]);
    let mut conflict = request.clone();
    conflict["inputs"]["prompt"] = json!("茶园晨光与茶叶露珠");
    assert!(store.create_local_execution(conflict).await.is_err());
    let mut invalid = request;
    invalid["request_id"] = json!("tea-http-json-invalid-parameter-20261031");
    invalid["inputs"]["resolution"] = json!("360p");
    assert!(store.create_local_execution(invalid).await.is_err());
    let id = execution["id"].as_str().ok_or("执行编号缺失")?.to_owned();
    loop {
        manifest["execution"] = execution.clone();
        std::fs::write(&path,serde_json::to_vec_pretty(&manifest)?)?;
        println!("HTTP JSON 执行 {}：{}",id,execution["status"]);
        match execution["status"].as_str() {
            Some("succeeded") => {assert_eq!(execution["waiting"],false);break;},
            Some("failed"|"cancelled"|"submission_unknown") => return Err(format!("执行未成功：{}",execution["error"]).into()),
            _=>{}
        }
        tokio::time::sleep(Duration::from_secs(8)).await;
        execution = store.get_local_execution(&id).await?;
    }
    println!("真实通用 HTTP 验收完成：{}",path.display());
    store.close().await;
    Ok(())
}
