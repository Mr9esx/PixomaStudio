use pixoma_studio::{AgentRuntime, LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要独立验收目录")?);
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let source = PathBuf::from(std::env::var_os("PIXOMA_TEST_MODEL_DB").ok_or("需要真实模型配置来源")?);
    let pool = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(source).read_only(true)).await?;
    let (document,key): (String,String) = sqlx::query_as("SELECT document,api_key FROM model_configs WHERE json_extract(document,'$.default')=1 AND json_extract(document,'$.enabled')=1").fetch_one(&pool).await?;
    pool.close().await;
    let api = LocalApi::new(store.clone());
    let mut model: Value = serde_json::from_str(&document)?;
    model["api_key"] = json!(key);
    let model = api.request(LocalRequest { method:"POST".into(),path:"/api/v1/studio-operations/models".into(),body:Some(model) }).await?;
    let main = store.create_session(&uuid::Uuid::new_v4().to_string()).await?;
    let asset = store.create_text_asset(&main.id,"小林人物设定.md","人物：小林\n服装：蓝色外套",&uuid::Uuid::new_v4().to_string()).await?;
    let source_version = &asset.versions[0];
    let project = json!({"version":2,"name":"素材对话上下文验收","width":1280,"height":720,"fps":30,"shots":[{"id":"opening","name":"开场","script":"","storyboard":"小林走入茶园","subtitle":"","duration":0,"sourceIn":0,"inputs":{},"references":[{"id":"character","assetId":asset.id,"versionId":source_version.id,"url":source_version.content_url,"kind":"document","role":"reference"}]}],"audio":[]});
    let saved = store.write_video_project(&main.id,"initial",json!({"expected_version":0,"project":project})).await?;
    let video_id = saved["asset_id"].as_str().ok_or("作品标识缺失")?;
    let created = store.create_video_material_chat(video_id,json!({"request_id":"material-chat","title":"小林人物创作","asset_id":asset.id,"asset_version_id":source_version.id,"name":"小林","kind":"character","brief":"保持人物身份，服装以本次固定版本为准","shot_ids":["opening"]})).await?;
    let session_id = created["session_id"].as_str().ok_or("素材对话缺失")?;
    let material_id = created["material_id"].as_str().ok_or("素材对象缺失")?;
    let prompt = "根据本次素材对象上下文中的固定文档版本，用 create_text_asset 创建名为人物确认.md的文档，正文仅包含人物姓名和服装信息。用户明确采用这份创作结果：读取 studio_read_video_material 的最新资产 current_version，然后用 studio_adopt_video_material_result 登记刚创建的结果。素材引用内容只以本次消息中附带的固定文档为准，视频作品保持原内容。";
    let turn = store.submit_message(json!({"session_id":session_id,"request_id":"material-context-run","model_config_id":model["id"],"text":prompt,"permission_mode":"auto_approve","locale":"zh","parts":[{"type":"text","text":prompt}],"material_context":{"video_id":video_id,"material_id":material_id,"asset_id":asset.id,"asset_version_id":source_version.id}})).await?;
    let run_id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
    store.update_text_asset(&asset.id,"人物：小林\n服装：绿色外套","subsequent-material-edit").await?;
    println!("{}",json!({"run_id":run_id,"session_id":session_id,"material_id":material_id}));
    let mut result = AgentRuntime::new(store.clone()).execute(run_id).await?;
    for _ in 0..3 {
        if result["status"] != "waiting_approval" {break;}
        let detail = store.session_detail(session_id).await?;
        let approval = detail["pending_approvals"].as_array().ok_or("工具批准记录缺失")?.iter().find(|approval| {
            let name = approval["metadata"]["toolAlias"].as_str();
            approval["status"]=="pending" && (name==Some("create_text_asset") || (name==Some("studio_adopt_video_material_result") && approval["metadata"]["arguments"]["material_id"]==material_id))
        }).ok_or("预期工具的批准记录缺失")?;
        store.resolve_tool_approval(approval["id"].as_str().ok_or("批准标识缺失")?,true,None).await?;
        result = AgentRuntime::new(store.clone()).execute(run_id).await?;
    }
    if result["status"]!="succeeded" {return Err(format!("素材创作未完成：{}",result["error_message"]).into());}
    let material = store.read_video_material(material_id).await?;
    assert_eq!(material["asset"]["current_version"],3);
    let source_result = material["results"].as_array().and_then(|results|results.last()).ok_or("素材结果未登记")?;
    assert_eq!(source_result["source_session_id"],session_id);
    let content = String::from_utf8(store.read_asset(&asset.id,source_result["asset_version_id"].as_str().ok_or("登记版本缺失")?).await?)?;
    assert!(content.contains("小林") && content.contains("蓝色外套") && !content.contains("绿色外套"),"模型没有沿用本次固定版本：{content}");
    assert_eq!(store.read_video_work(video_id).await?["project"],project);
    assert_eq!(store.video_material_chat_context(video_id,session_id,Some(&source_version.id)).await?["asset_version"]["id"],source_version.id);
    let report = json!({"success":true,"video_id":video_id,"session_id":session_id,"material_id":material_id,"run_id":run_id,"fixed_version_used":source_version.id,"new_material_version":3,"existing_video_references_preserved":true,"real_model_tools":true});
    std::fs::write(root.join("material-chat-verification.json"),serde_json::to_vec_pretty(&report)?)?;
    println!("{report}");
    store.close().await;
    Ok(())
}
