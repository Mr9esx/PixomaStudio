use gray_matter::{Matter, engine::YAML};
use pixoma_studio::{AgentRuntime, LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要验收数据目录")?);
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let source = PathBuf::from(std::env::var_os("PIXOMA_TEST_MODEL_DB").ok_or("需要模型配置来源")?);
    let connection = sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(source).read_only(true)).await?;
    let (document, credential): (String, String) = sqlx::query_as("SELECT document,api_key FROM model_configs WHERE json_extract(document,'$.default')=1 AND json_extract(document,'$.enabled')=1").fetch_one(&connection).await?;
    connection.close().await;
    let mut configuration: Value = serde_json::from_str(&document)?;
    configuration["api_key"] = json!(credential);
    let model = api.request(LocalRequest { method: "POST".into(), path: "/api/v1/studio-operations/models".into(), body: Some(configuration) }).await?;
    let content = include_str!("../../src/features/studio/skills/pixoma-studio-video-creator/SKILL.md");
    let metadata = Matter::<YAML>::new().parse::<Value>(content)?.data.ok_or("技能元数据缺失")?;
    let skill = api.request(LocalRequest { method: "POST".into(), path: "/api/v1/studio/skills".into(), body: Some(json!({"name":metadata["name"],"description":metadata["description"],"files":[{"path":"SKILL.md","content":content}],"enabled":true,"version":"1.0.0"})) }).await?;
    let session = store.create_session(&format!("video-creator-{}",uuid::Uuid::new_v4())).await?;
    let initial = json!({"version":2,"name":"茶园晨光","width":1280,"height":720,"fps":30,"shots":[],"audio":[],"script":{"revision":0,"blocks":[]}});
    let record = store.write_video_project(&session.id,"initial",json!({"expected_version":0,"project":initial})).await?;
    let video_id = record["asset_id"].as_str().ok_or("作品标识缺失")?;
    let mut receipts = Vec::new();
    for (scope,prompt) in [
        ("script","创作完整脚本：小林清晨走进茶园，采下一片茶叶，回家泡茶。安静自然，无对白。以三个有序文本块保存提案。读取 studio_read_video_project 后调用 studio_propose_video_script。"),
        ("storyboard","根据已采用的完整脚本生成三个分镜，描述人物动作、场景和镜头，引用脚本文本块。读取 studio_read_video_project 后调用 studio_propose_video_storyboard 保存新增分镜提案。"),
    ] {
        let current = store.read_video_work(video_id).await?;
        let turn = store.submit_message(json!({"session_id":session.id,"request_id":format!("{}:{scope}",session.id),"model_config_id":model["id"],"text":prompt,"permission_mode":"auto_approve","locale":"zh","skill_ids":[skill["id"]],"parts":[{"type":"skill_ref","skill_id":skill["id"],"version":skill["version"],"name":skill["name"]},{"type":"text","text":prompt}],"video_edit_context":{"scope":scope,"video_id":video_id,"project":current["project"]}})).await?;
        let run_id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
        println!("{}",json!({"stage":scope,"run_id":run_id}));
        let result = AgentRuntime::new(store.clone()).execute(run_id).await?;
        if result["status"] != "succeeded" { return Err(format!("创作请求未成功：{}",result["error_message"]).into()); }
        let saved = store.read_video_work(video_id).await?;
        assert_eq!(saved["project"]["shots"],current["project"]["shots"]);
        assert_eq!(saved["project"]["script"],current["project"]["script"]);
        let proposals = saved["project"]["proposals"].as_array().ok_or("创作提案缺失")?;
        let proposal = proposals.iter().find(|item|item["kind"]==scope).ok_or("目标创作提案缺失")?;
        if scope == "script" {
            assert_eq!(proposal["script"]["blocks"].as_array().ok_or("脚本文本块缺失")?.len(),3);
            let mut adopted = saved["project"].clone();
            adopted["script"] = proposal["script"].clone();
            adopted["proposals"] = json!([]);
            store.write_video_work(video_id,&session.id,"adopt-script",json!({"expected_version":saved["asset_version"],"project":adopted})).await?;
        } else {
            let changes = proposal["changes"].as_array().ok_or("分镜提案缺失")?;
            assert_eq!(changes.len(),3);
            for change in changes {
                assert_eq!(change["action"],"add");
                assert!(change["storyboard"].as_str().is_some_and(|text|text.chars().count()>10));
                assert!(change["sources"].as_array().is_some_and(|sources|!sources.is_empty()));
            }
        }
        receipts.push(json!({"scope":scope,"run_id":run_id,"proposal":proposal}));
        println!("{}",json!({"stage":scope,"status":"verified"}));
    }
    std::fs::write(root.join("video-creator-verification.json"),serde_json::to_vec_pretty(&json!({"session_id":session.id,"video_id":video_id,"cases":receipts}))?)?;
    store.close().await;
    Ok(())
}
