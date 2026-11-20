use pixoma_studio::{LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("PIXOMA_VIDEO_DATA_ROOT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache/real-video-20261031"));
    std::fs::create_dir_all(&root)?;
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let path = root.join("video-acceptance.json");
    let mut manifest: Value = if path.exists() {serde_json::from_slice(&std::fs::read(&path)?)?} else {
        let session = store.create_session("tea-video-acceptance-20261031").await?;
        let project = store.create_project("山间茶事 · 真实视频验收").await?;
        store.move_session(&session.id,Some(&project.id)).await?;
        let connections = store.list_generation_connections().await?;
        let connection = if let Some(connection) = connections.as_array().ok_or("连接列表无效")?.iter().find(|connection| connection["name"] == "Seedance 验收连接") { connection.clone() } else {
            let key = std::env::var("PIXOMA_SEEDANCE_KEY")?;
            store.save_generation_connection(json!({"name":"Seedance 验收连接","provider":"seedance","api_key":key})).await?
        };
        let templates = api.request(LocalRequest {method:"GET".into(),path:"/api/v1/studio/generation-templates".into(),body:None}).await?;
        let mut definition = templates[0]["definition"].clone();
        definition["execution"]["connection_id"] = connection["id"].clone();
        let workflow = store.create_workflow(json!({"name":"Seedance 2 · 茶品牌分镜","category":"video","tags":["茶品牌","真实验收"],"enabled":true,"draft":definition})).await?;
        let workflow_id = workflow["id"].as_str().ok_or("工作流编号缺失")?;
        let version = store.create_workflow_version(workflow_id,json!({"expected_revision":1,"version":"1.0.0","notes":"真实视频生成验收"})).await?;
        store.set_default_workflow_version(workflow_id,version["id"].as_str().ok_or("版本编号缺失")?).await?;
        json!({"session_id":session.id,"project_id":project.id,"workflow_id":workflow_id,"workflow_version_id":version["id"],"executions":[]})
    };
    std::fs::write(&path,serde_json::to_vec_pretty(&manifest)?)?;
    let prompts = [
        "茶品牌短片分镜，清晨自然光，微距拍摄新鲜茶叶，翠绿色叶片上的透明露珠缓缓滑动，背景茶园柔和模糊，镜头轻缓横移，真实电影摄影，画面安静，只有清晨鸟鸣和轻微风声，无字幕无文字无标志。",
        "茶品牌短片分镜，木质茶桌上的白瓷盖碗，少量干茶叶缓慢洒入盖碗，特写俯拍，自然晨光从窗边照入，暖灰与茶绿色调，镜头稳定，真实电影摄影，保留轻柔茶叶触碰瓷器的声音，无字幕无文字无标志。",
        "茶品牌短片分镜，热水从画面上方缓慢注入白瓷茶盏，琥珀色茶汤泛起清晰涟漪，蒸汽轻轻升起，近景固定镜头，背景朦胧木桌，柔和自然光，真实电影摄影，只有细小水声，无字幕无文字无标志。",
        "茶品牌短片结尾分镜，一盏琥珀色热茶静置在古朴木桌上，旁边一枝绿色茶叶，背景窗外是清晨山间，蒸汽缓慢升腾，镜头轻缓推进，自然光温暖而克制，真实电影摄影，只有自然环境声，无字幕无文字无标志。",
    ];
    for (index,prompt) in prompts.iter().enumerate() {
        let request = json!({"request_id":format!("tea-seedance-20261031-shot-{}",index+2),"workflow_id":manifest["workflow_id"],"version_id":manifest["workflow_version_id"],"session_id":manifest["session_id"],"inputs":{"prompt":prompt,"resolution":"720p","ratio":"16:9","duration":4,"generate_audio":true}});
        let mut execution = store.create_local_execution(request).await?;
        let id = execution["id"].as_str().ok_or("执行编号缺失")?.to_owned();
        execution = store.get_local_execution(&id).await?;
        loop {
            if let Some(entries) = manifest["executions"].as_array_mut() {
                if let Some(entry) = entries.iter_mut().find(|entry| entry["id"] == id) {*entry = execution.clone();} else {entries.push(execution.clone());}
            }
            std::fs::write(&path,serde_json::to_vec_pretty(&manifest)?)?;
            println!("分镜 {}：{}，任务 {}",index+2,execution["status"],id);
            match execution["status"].as_str() {
                Some("succeeded") => break,
                Some("failed"|"cancelled"|"submission_unknown") => return Err(format!("生成未成功：{}",execution["error"]).into()),
                _ => {}
            }
            tokio::time::sleep(Duration::from_secs(8)).await;
            execution = store.get_local_execution(&id).await?;
        }
    }
    println!("四段真实视频已登记，清单：{}",path.display());
    store.close().await;
    Ok(())
}
