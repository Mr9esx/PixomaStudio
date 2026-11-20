use tauri::{Listener, Manager};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要验收数据目录")?);
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join("video-acceptance.json"))?)?;
    let h3: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join("h3-manifest.json"))?)?;
    let registration = tauri::async_runtime::block_on(async {
        let store = pixoma_studio::Store::open(&root.join("studio-standalone.sqlite")).await?;
        let registration = store.rotate_compute_node_credential(h3["node_id"].as_str().ok_or(pixoma_studio::Error::State("H3节点标识缺失"))?).await?;
        store.close().await;
        Ok::<_,pixoma_studio::Error>(registration)
    })?;
    let mode = std::env::args().nth(1).unwrap_or_else(||"verify".to_owned());
    if !["verify","browse"].contains(&mode.as_str()) {return Err("视频窗口模式无效".into());}
    let script = format!("window.__PIXOMA_VIDEO_SESSION__={}; window.__PIXOMA_VIDEO_MODE__={}; {}", manifest["session_id"],serde_json::to_string(&mode)?, include_str!("native_video.js"));
    let started = AtomicBool::new(false);
    let agent: Arc<Mutex<Option<std::process::Child>>> = Arc::new(Mutex::new(None));
    let agent_directory = root.join("h3-agent");
    let agent_binary = root.parent().ok_or("缓存目录缺失")?.join("edge-agent");
    let mut context = pixoma_studio::desktop_context();
    context.config_mut().identifier = "com.pixoma.studio.video-acceptance".into();
    context.config_mut().app.windows[0].background_throttling = Some(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    let app = pixoma_studio::desktop_builder().on_page_load(move |webview, payload| {
        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) && !started.swap(true, Ordering::SeqCst) {
            webview.eval(&script).expect("启动视频编辑器验收失败");
        }
    }).build(context)?;
    app.listen("studio:video-progress", move |event| {
        println!("{}", event.payload());
        let payload: serde_json::Value = serde_json::from_str(event.payload()).expect("视频验收记录格式无效");
        if payload["stage"] == "complete" || payload["stage"] == "failed" {
            std::fs::write(root.join("native-video-result.json"),serde_json::to_vec_pretty(&payload).expect("视频验收记录无法编码")).expect("视频验收记录无法保存");
        }
    });
    let run_agent = agent.clone();
    app.run(move |app, event| {
        if matches!(event, tauri::RunEvent::Ready) {
            *run_agent.lock().expect("Agent状态无法读取") = Some(std::process::Command::new(&agent_binary)
                .env("CONTROL_PLANE_URL","http://127.0.0.1:19429")
                .env("EDGE_ID",h3["node_id"].as_str().expect("H3节点标识缺失"))
                .env("AGENT_TOKEN",registration["agent_token"].as_str().expect("节点凭据缺失"))
                .env("EDGE_DATA_DIR",&agent_directory)
                .env_remove("HTTP_PROXY").env_remove("HTTPS_PROXY").env_remove("ALL_PROXY")
                .env_remove("http_proxy").env_remove("https_proxy").env_remove("all_proxy")
                .spawn().expect("启动验收Agent失败"));
            let window = app.get_webview_window("main").expect("Studio 主窗口不存在");
            window.show().expect("显示视频编辑器失败");
        }
    });
    if let Some(mut child) = agent.lock().expect("Agent状态无法读取").take() { child.kill()?; child.wait()?; }
    Ok(())
}
