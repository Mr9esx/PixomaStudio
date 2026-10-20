use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::Listener;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let configuration: serde_json::Value =
        serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
    let account = serde_json::json!({
        "username": std::env::var("PIXOMA_TEST_STUDIO_USERNAME")?,
        "password": std::env::var("PIXOMA_TEST_STUDIO_PASSWORD")?,
    });
    let other = serde_json::json!({
        "username": std::env::var("PIXOMA_TEST_OTHER_USERNAME")?,
        "password": std::env::var("PIXOMA_TEST_OTHER_PASSWORD")?,
    });
    let connector = serde_json::json!({
        "name": "真实 MCP 检查", "url": std::env::var("PIXOMA_TEST_MCP_URL")?,
        "credential": std::env::var("PIXOMA_TEST_MCP_TOKEN")?, "enabled": true, "policy": "approval",
    });
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let succeeded = Arc::new(AtomicBool::new(false));
    let script = format!(
        "window.__PIXOMA_MODEL_CHECK__ = {}; window.__PIXOMA_ACCOUNT_CHECK__ = {}; window.__PIXOMA_OTHER_CHECK__ = {}; window.__PIXOMA_MCP_CHECK__ = {}; {}",
        configuration,
        account,
        other,
        connector,
        include_str!("native_journey_bootstrap.js")
    );
    let app = pixoma_studio::desktop_builder()
        .on_page_load(move |webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
                && !started.swap(true, Ordering::SeqCst)
            {
                webview.eval(&script).expect("启动原生联调脚本失败");
            }
        })
        .build(pixoma_studio::desktop_context())?;
    app.listen("studio:journey-progress", |event| {
        println!("{}", event.payload())
    });
    let handle = app.handle().clone();
    let result = succeeded.clone();
    let completion = finished.clone();
    app.listen("studio:journey-result", move |event| {
        let payload: serde_json::Value =
            serde_json::from_str(event.payload()).expect("联调结果格式无效");
        result.store(payload["success"] == true, Ordering::SeqCst);
        completion.store(true, Ordering::SeqCst);
        println!("{payload}");
        handle.exit(if payload["success"] == true { 0 } else { 1 });
    });
    let timeout_handle = app.handle().clone();
    let timeout_finished = finished.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(300));
        if !timeout_finished.load(Ordering::SeqCst) {
            eprintln!("原生联调超过 300 秒");
            timeout_handle.exit(1);
        }
    });
    app.run(|_, _| {});
    if !finished.load(Ordering::SeqCst) {
        return Err("原生联调未完成".into());
    }
    if !succeeded.load(Ordering::SeqCst) {
        return Err("原生联调失败".into());
    }
    Ok(())
}
