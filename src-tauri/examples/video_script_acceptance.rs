use gray_matter::{Matter, engine::YAML};
use pixoma_studio::{AgentRuntime, LocalApi, LocalRequest, Store};
use serde_json::{Value, json};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要验收数据目录")?);
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let api = LocalApi::new(store.clone());
    let content = include_str!("../../src/features/studio/skills/video-script/SKILL.md");
    let manifest = Matter::<YAML>::new().parse::<Value>(content)?.data.ok_or("技能元数据缺失")?;
    let skills = api.request(LocalRequest { method: "GET".into(), path: "/api/v1/studio/skills".into(), body: None }).await?;
    let skill = if let Some(skill) = skills.as_array().ok_or("技能列表无效")?.iter().find(|skill| skill["name"] == manifest["name"]) {
        let path = format!("/api/v1/studio/skills/{}",skill["id"].as_str().ok_or("技能标识缺失")?);
        let existing = api.request(LocalRequest { method: "GET".into(), path: path.clone(), body: None }).await?;
        let files = json!([{"path":"SKILL.md","content":content}]);
        if existing["files"] == files {existing} else {
            let mut version = semver::Version::parse(existing["version"].as_str().ok_or("技能版本缺失")?)?;
            version.patch += 1;
            api.request(LocalRequest { method: "PATCH".into(), path, body: Some(json!({"name":manifest["name"],"description":manifest["description"],"files":files,"version":version.to_string(),"updated_at":existing["updated_at"],"notes":"整片与分镜剧本生成"})) }).await?
        }
    } else {
        api.request(LocalRequest { method: "POST".into(), path: "/api/v1/studio/skills".into(), body: Some(json!({"name":manifest["name"],"description":manifest["description"],"prompt":"","files":[{"path":"SKILL.md","content":content}],"enabled":true,"version":"1.0.0"})) }).await?
    };
    let models = api.request(LocalRequest { method: "GET".into(), path: "/api/v1/studio/models".into(), body: None }).await?;
    let model = models.as_array().ok_or("模型列表无效")?.iter().find(|model| model["default"] == true && model["enabled"] == true).ok_or("默认模型不可用")?;
    let session = store.create_session(&format!("video-script-{}", uuid::Uuid::new_v4())).await?;
    let project = json!({"version":2,"name":"剧本生成验收","width":1280,"height":720,"fps":30,"audio":[],"shots":[
        {"id":"opening","name":"开场","script":"","storyboard":"清晨远景","references":[],"subtitle":"山间晨光","duration":3,"sourceIn":0,"inputs":{}},
        {"id":"ending","name":"结尾","script":"人物望向窗外","storyboard":"近景","references":[],"subtitle":"新的一天","duration":4,"sourceIn":0,"inputs":{}}
    ]});
    store.write_video_project(&session.id,"script-initial",json!({"expected_version":0,"project":project})).await?;
    let mut receipts = Vec::new();
    for (index, idea) in ["清晨茶园薄雾散开，安静自然，无对白。", "人物听见远处脚步声，停下倒茶的动作，期待来客，无对白。", "用杯口升起的蒸汽表现温暖，保持三秒镜头，不出现人物。"].iter().enumerate() {
        let current = store.read_video_project(&session.id).await?;
        let prompt = format!("使用 video-script Skill 为 opening 分镜生成剧本。想法：{idea}。计划时长三秒。读取最新视频项目，调用 studio_update_video_shot 只修改 opening 的 script，保留其他内容。不要生成视频或导出。");
        let turn = store.submit_message(json!({"session_id":session.id,"request_id":format!("script:{}:{index}",session.id),"model_config_id":model["id"],"text":prompt,"permission_mode":"auto_approve","locale":"zh","skill_ids":[skill["id"]],"parts":[{"type":"skill_ref","skill_id":skill["id"],"version":skill["version"],"name":skill["name"]},{"type":"text","text":prompt}],"video_edit_context":{"shot_id":"opening","script":current["project"]["shots"][0]["script"]}})).await?;
        let run_id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
        println!("{}", json!({"stage":"模型正在生成剧本","case":index,"run_id":run_id}));
        let outcome = AgentRuntime::new(store.clone()).execute(run_id).await?;
        if outcome["status"] != "succeeded" {return Err(format!("模型请求未完成：{}",outcome["error_message"]).into());}
        let saved = store.read_video_project(&session.id).await?;
        assert!(saved["project"]["shots"][0]["script"].as_str().is_some_and(|script|script.chars().count()>10));
        let mut expected = current["project"].clone(); expected["shots"][0]["script"] = saved["project"]["shots"][0]["script"].clone();
        assert_eq!(saved["project"],expected);
        receipts.push(json!({"case":index,"run_id":run_id,"script":saved["project"]["shots"][0]["script"],"scope_retained":true}));
    }
    let whole_session = store.create_session(&format!("video-whole-script-{}",uuid::Uuid::new_v4())).await?;
    let baseline = store.read_video_project(&whole_session.id).await?;
    let prompt = "使用 video-script Skill 从空作品创作整片剧本，scope=project。主题：清晨人物走入茶园、采下一片嫩叶、捧起热茶望向朝阳。生成恰好三个有序分镜，时长分别为 3、4、5 秒。每个分镜包含具体剧情 script 和独立画面描述 storyboard。调用 studio_write_video_project 一次写入完整项目。保留原始宽度、高度和帧率；字幕保持空，inputs 与 references 保持空，不生成媒体。";
    let turn = store.submit_message(json!({"session_id":whole_session.id,"request_id":format!("whole-script:{}",whole_session.id),"model_config_id":model["id"],"text":prompt,"permission_mode":"auto_approve","locale":"zh","skill_ids":[skill["id"]],"parts":[{"type":"skill_ref","skill_id":skill["id"],"version":skill["version"],"name":skill["name"]},{"type":"text","text":prompt}],"video_edit_context":{"scope":"project","project":baseline["project"]}})).await?;
    let run_id = turn["run"]["id"].as_str().ok_or("运行标识缺失")?;
    println!("{}",json!({"stage":"模型正在生成整片剧本","run_id":run_id}));
    let outcome = AgentRuntime::new(store.clone()).execute(run_id).await?;
    if outcome["status"] != "succeeded" {return Err(format!("整片剧本请求未完成：{}",outcome["error_message"]).into());}
    let saved = store.read_video_project(&whole_session.id).await?;
    let shots = saved["project"]["shots"].as_array().ok_or("分镜列表缺失")?;
    assert_eq!(shots.len(),3);
    assert_eq!(shots.iter().map(|shot|shot["duration"].as_f64().unwrap()).collect::<Vec<_>>(),vec![3.0,4.0,5.0]);
    for shot in shots {
        assert!(shot["script"].as_str().is_some_and(|script|script.chars().count()>10));
        assert!(shot["storyboard"].as_str().is_some_and(|text|!text.is_empty()));
        assert_eq!(shot["subtitle"],""); assert_eq!(shot["inputs"],json!({})); assert_eq!(shot["references"],json!([]));
        assert!(shot.get("media").is_none());
    }
    for field in ["width","height","fps"] {assert_eq!(saved["project"][field],baseline["project"][field]);}
    let receipt = json!({"success":true,"session_id":session.id,"model_id":model["id"],"skill_id":skill["id"],"skill_version":skill["version"],"cases":receipts,"whole_project":{"session_id":whole_session.id,"run_id":run_id,"project":saved["project"]}});
    std::fs::write(root.join("video-script-verification.json"),serde_json::to_vec_pretty(&receipt)?)?;
    println!("{receipt}");
    store.close().await;
    Ok(())
}
