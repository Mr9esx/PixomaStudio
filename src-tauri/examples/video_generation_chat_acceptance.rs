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
    let definition: Value = serde_json::from_str(include_str!("../../docs/video-generation/h3/tea-h3-workflow.json"))?;
    let workflow = store.create_workflow(json!({"name":"停用工作流提交验收","category":"video","enabled":false,"draft":definition})).await?;
    let workflow_id = workflow["id"].as_str().ok_or("工作流标识缺失")?;
    let version = store.create_workflow_version(workflow_id,json!({"expected_revision":1,"version":"1.0.0"})).await?;
    let session = store.create_session(&uuid::Uuid::new_v4().to_string()).await?;
    let shot = json!({"id":"opening","name":"茶园晨光","script":"","storyboard":"晨光穿过茶树","references":[],"subtitle":"","duration":0,"sourceIn":0,"workflowId":workflow_id,"versionId":version["id"],"inputs":{"prompt":"晨光中的茶园"}});
    let saved = store.write_video_project(&session.id,"initial",json!({"expected_version":0,"project":{"version":2,"name":"Chat 生成记录验收","width":1280,"height":720,"fps":30,"shots":[shot],"audio":[]}})).await?;
    let video_id = saved["asset_id"].as_str().ok_or("作品标识缺失")?;
    let mut original: Option<Value> = None;
    let mut receipts = Vec::new();
    for action in ["generate","retry"] {
        let prompt = if let Some(record) = &original {
            format!("使用 studio_retry_video_generation，按视频作品 {video_id} 的生成记录 {} 的原输入再次提交。工作流保持当前状态。随后调用 studio_list_video_generations 查询实际结果，准确说明返回的失败原因。",record["id"].as_str().ok_or("记录标识缺失")?)
        } else {
            format!("对视频作品 {video_id} 的分镜 opening 使用当前保存设置提交一次生成。调用 studio_read_video_project 读取最新修订，然后调用 studio_generate_video_shot。工作流当前停用，这是本次需要验证的失败流程，保持当前配置，仍然提交以保留失败记录。随后调用 studio_list_video_generations 查询实际状态，准确说明失败原因。")
        };
        let turn = store.submit_message(json!({"session_id":session.id,"request_id":format!("{}:{action}",session.id),"model_config_id":model["id"],"text":prompt,"permission_mode":"auto_approve","locale":"zh","parts":[{"type":"text","text":prompt}]})).await?;
        let run_id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
        println!("{}",json!({"action":action,"run_id":run_id}));
        let mut result = AgentRuntime::new(store.clone()).execute(run_id).await?;
        if result["status"] == "waiting_approval" {
            let detail = store.session_detail(&session.id).await?;
            let approvals = detail["pending_approvals"].as_array().ok_or("工具批准记录缺失")?;
            let expected_tool = if action=="generate" {"studio_generate_video_shot"} else {"studio_retry_video_generation"};
            let approval = approvals.iter().find(|approval|approval["metadata"]["toolAlias"]==expected_tool && approval["metadata"]["arguments"]["video_id"]==video_id && approval["status"]=="pending").ok_or("预期工具的批准记录缺失")?;
            store.resolve_tool_approval(approval["id"].as_str().ok_or("批准标识缺失")?,true,None).await?;
            result = AgentRuntime::new(store.clone()).execute(run_id).await?;
        }
        if result["status"] != "succeeded" {return Err(format!("Chat 验收运行未完成：{}",result["error_message"]).into());}
        let records = store.list_video_generations(video_id).await?;
        let records = records.as_array().ok_or("生成记录缺失")?;
        assert_eq!(records.len(),if action=="generate" {1} else {2});
        let record = if let Some(original) = &original {
            let retry = records.iter().find(|record|record["retry_of"]==original["id"]).ok_or("Chat 没有使用原记录重试")?;
            assert_eq!(retry["snapshot"],original["snapshot"]);
            retry
        } else {&records[0]};
        assert_eq!(record["status"],"submission_failed");
        assert!(record["submission_error"].as_str().is_some_and(|message|message.contains("停用")));
        if original.is_none() {original=Some(record.clone());}
        receipts.push(json!({"action":action,"run_id":run_id,"generation_id":record["id"],"status":record["status"]}));
        println!("{}",json!({"action":action,"verified":true}));
    }
    let result = json!({"success":true,"video_id":video_id,"session_id":session.id,"cases":receipts,"real_model_tools":true,"submission_failure_preserved":true,"retry_inputs_preserved":true});
    std::fs::write(root.join("chat-verification.json"),serde_json::to_vec_pretty(&result)?)?;
    println!("{result}");
    store.close().await;
    Ok(())
}
