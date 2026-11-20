use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tauri::{Listener, Manager};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let workflow = std::path::PathBuf::from(args.next().ok_or("需要工作流文件路径")?);
    let binary = std::path::PathBuf::from(args.next().ok_or("需要 Agent 可执行文件路径")?);
    let root = std::path::PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要独立检查目录")?);
    let input = serde_json::json!({
        "filename": workflow.file_name().ok_or("工作流名称缺失")?.to_str().ok_or("工作流名称编码无效")?,
        "content": std::fs::read_to_string(&workflow)?,
    });
    let script = format!("window.__PIXOMA_WORKFLOW_INPUT__ = {input}; {}", include_str!("native_workflow.js"));
    let started = AtomicBool::new(false);
    let succeeded = Arc::new(AtomicBool::new(false));
    let agent: Arc<Mutex<Option<std::process::Child>>> = Arc::new(Mutex::new(None));
    let mut context = pixoma_studio::desktop_context();
    context.config_mut().identifier = "com.pixoma.studio.workflow-check".into();
    context.config_mut().app.windows[0].background_throttling = Some(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    let app = pixoma_studio::desktop_builder().on_page_load(move |webview, payload| {
        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) && !started.swap(true, Ordering::SeqCst) {
            webview.eval(&script).expect("启动工作流页面检查失败");
        }
    }).build(context)?;
    let child = agent.clone();
    let data_directory = root.join("agent");
    let listen = std::env::var("PIXOMA_STUDIO_AGENT_LISTEN")?.parse::<std::net::SocketAddr>()?;
    let control_url = format!("http://{listen}");
    app.listen("studio:workflow-agent", move |event| {
        let input: serde_json::Value = serde_json::from_str(event.payload()).expect("Agent 连接参数无效");
        let mut current = child.lock().expect("Agent 进程状态无法读取");
        assert!(current.is_none(), "Agent 已经启动");
        *current = Some(std::process::Command::new(&binary)
            .env("CONTROL_PLANE_URL", &control_url)
            .env("EDGE_ID", input["node"]["id"].as_str().expect("节点标识缺失"))
            .env("AGENT_TOKEN", input["agent_token"].as_str().expect("节点凭据缺失"))
            .env("EDGE_DATA_DIR", &data_directory)
            .env_remove("HTTP_PROXY").env_remove("HTTPS_PROXY").env_remove("ALL_PROXY")
            .env_remove("http_proxy").env_remove("https_proxy").env_remove("all_proxy")
            .spawn().expect("启动真实 Agent 失败"));
    });
    app.listen("studio:workflow-progress", |event| { eprintln!("工作流页面检查：{}", event.payload()); });
    let result = succeeded.clone();
    let handle = app.handle().clone();
    app.listen("studio:workflow-result", move |event| {
        let payload: serde_json::Value = serde_json::from_str(event.payload()).expect("工作流检查结果无效");
        println!("{payload}");
        std::fs::write(root.join("result.json"), serde_json::to_vec_pretty(&payload).expect("检查记录无法编码")).expect("检查记录无法保存");
        let success = payload["success"] == true;
        result.store(success, Ordering::SeqCst);
        handle.exit(if success { 0 } else { 1 });
    });
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(180));
        handle.exit(1);
    });
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::Ready) {
            let window = app.get_webview_window("main").expect("Studio 主窗口不存在");
            window.show().expect("显示 Studio 窗口失败");
            window.set_focus().expect("激活 Studio 窗口失败");
        }
    });
    if let Some(mut child) = agent.lock().expect("Agent 状态无法读取").take() {
        child.kill()?;
        child.wait()?;
    }
    if !succeeded.load(Ordering::SeqCst) { return Err("桌面工作流验收失败".into()); }
    Ok(())
}
