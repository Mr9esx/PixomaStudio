use pixoma_studio::{AgentRuntime, LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要验收数据目录")?);
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let manifest: Value = serde_json::from_slice(&std::fs::read(root.join("video-acceptance.json"))?)?;
    let models = api.request(LocalRequest {method:"GET".into(),path:"/api/v1/studio/models".into(),body:None}).await?;
    let model = if let Some(model) = models.as_array().ok_or("模型列表无效")?.iter().find(|model| model["name"] == "DeepSeek V4.1 Flash · 视频创作") {model.clone()} else {
        api.request(LocalRequest {method:"POST".into(),path:"/api/v1/studio-operations/models".into(),body:Some(json!({
            "name":"DeepSeek V4.1 Flash · 视频创作","protocol":"openai_chat_compatible","base_url":"https://api.deepseek.com","model":"deepseek-flash",
            "api_key":std::env::var("PIXOMA_DEEPSEEK_KEY")?,"enabled":true,"agent_enabled":true,"default":true,
            "limits":{"context_window_tokens":128000,"max_input_tokens":96000,"max_output_tokens":16000},
            "thinking":{"enabled":false},"capabilities":{"tools":true,"vision":true,"image_output":false,"streaming":true}
        }))}).await?
    };
    let stage = std::env::args().nth(1).ok_or("需要验收阶段：create、apply、generate 或 export")?;
    if stage == "restore-media" {
        let current = store.read_video_project(manifest["session_id"].as_str().ok_or("会话编号缺失")?).await?;
        let previous: Value = serde_json::from_slice(&std::fs::read(root.join("chat-apply.json"))?)?;
        let mut project = current["project"].clone();
        let first = &previous["project"]["project"]["shots"][0];
        for key in ["media","executionId"] {project["shots"][0][key] = first[key].clone();}
        store.write_video_project(manifest["session_id"].as_str().ok_or("会话编号缺失")?,"restore-retained-video",json!({"expected_version":current["asset_version"],"project":project})).await?;
        println!("已保留当前剧本并恢复采用视频");
        store.close().await;
        return Ok(());
    }
    let h3: Value = serde_json::from_slice(&std::fs::read(root.join("h3-manifest.json"))?)?;
    let message = match stage.as_str() {
        "create" => format!("创建名为《山间茶事》的15秒茶品牌视频，1280×720，30fps，五个分镜，每个剪辑时长3秒。分镜依次为雾起山间、一叶新茶、茶入盖碗、热水入盏、茶香入席，编写各自的剧本和字幕。第一镜选择H3工作流 {} 版本 {}，其余选择Seedance工作流 {} 版本 {}。读取工作流要求，为每镜保存有效生成参数，H3用608×352、124帧、seed为2026103104，Seedance用720p、16:9、4秒、有声音。仅创建并保存项目，不执行生成。",h3["workflow_id"],h3["version_id"],manifest["workflow_id"],manifest["workflow_version_id"]),
        "apply" => format!("读取当前视频项目，将以下五次成功执行的视频结果依次采用到五个分镜，剪辑开始位置均为0秒，剪辑时长均为3秒。每次采用前读取最新项目版本。第一镜执行 {}，其余四镜执行 {}。保持剧本、字幕和工作流设置。",h3["execution_id"],serde_json::to_string(&manifest["executions"].as_array().ok_or("执行列表无效")?.iter().map(|execution|execution["id"].clone()).collect::<Vec<_>>())?),
        "generate" => "与第一个分镜单独对话：保持其他四镜内容不变，把第一个分镜剧本改成清晨茶园中层叠山峦与轻柔晨雾，镜头缓慢推进，安静写实，无文字；更新对应prompt，使用该分镜已选择的H3工作流重新生成视频。完成后采用新结果，保留原来的视频版本。剪辑时长保持3秒。".into(),
        "export" => "读取当前项目，确认五个分镜均已采用视频、总时长15秒，然后导出完整MP4成片。".into(),
        _ => return Err("验收阶段无效".into()),
    };
    let turn = store.submit_message(json!({"session_id":manifest["session_id"],"request_id":format!("real-video-chat-20261031-{stage}"),"model_config_id":model["id"],"text":message,"permission_mode":"auto_approve","locale":"zh"})).await?;
    let id = turn["run"]["id"].as_str().ok_or("Chat运行编号缺失")?;
    let runtime = AgentRuntime::new(store.clone());
    let outcome = runtime.execute(id).await?;
    let events = store.run_events_after(id,0).await?;
    let text: String = events.iter().filter(|event|event["type"] == "TEXT_MESSAGE_CONTENT").filter_map(|event|event["payload"]["delta"].as_str()).collect();
    let project = store.read_video_project(manifest["session_id"].as_str().ok_or("会话编号缺失")?).await?;
    let receipt = json!({"stage":stage,"run_id":id,"status":outcome["status"],"error":outcome["error_message"],"text":text,"project":project});
    std::fs::write(root.join(format!("chat-{stage}.json")),serde_json::to_vec_pretty(&receipt)?)?;
    println!("{}",serde_json::to_string_pretty(&receipt)?);
    if outcome["status"] != "succeeded" {return Err("真实Chat流程没有完成".into());}
    if stage == "create" {
        let shots = project["project"]["shots"].as_array().ok_or("分镜列表缺失")?;
        assert_eq!(shots.len(),5);
        assert_eq!(shots.iter().map(|shot|shot["duration"].as_f64().expect("分镜时长缺失")).sum::<f64>(),15.0);
    }
    store.close().await;
    Ok(())
}
