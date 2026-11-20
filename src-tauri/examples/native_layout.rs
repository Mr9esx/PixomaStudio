use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Listener, Manager};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script_path = std::env::var_os("PIXOMA_LAYOUT_SCRIPT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/examples/native_layout.js"
            ))
        });
    let mut script = std::fs::read_to_string(script_path)?;
    if let Some(workflow) = std::env::var_os("PIXOMA_LAYOUT_WORKFLOW") {
        let document = std::fs::read_to_string(workflow)?;
        let workflow: serde_json::Value = serde_json::from_str(&document)?;
        script = format!("window.__PIXOMA_LAYOUT_WORKFLOW__={workflow};{script}");
    }
    let timeout_seconds = match std::env::var("PIXOMA_LAYOUT_TIMEOUT_SECONDS") {
        Ok(value) => value.parse::<u64>()?,
        Err(std::env::VarError::NotPresent) => 180,
        Err(error) => return Err(error.into()),
    };
    let started = AtomicBool::new(false);
    let finished = Arc::new(AtomicBool::new(false));
    let succeeded = Arc::new(AtomicBool::new(false));
    let mut context = pixoma_studio::desktop_context();
    if let Some(url) = std::env::var_os("PIXOMA_LAYOUT_WEB_URL") {
        context.config_mut().build.dev_url = Some(url.to_str().ok_or("检查页面地址编码无效")?.parse()?);
    }
    context.config_mut().identifier = "com.pixoma.studio.layout-check".into();
    if let Some(identifier) = std::env::var_os("PIXOMA_LAYOUT_IDENTIFIER") {
        context.config_mut().identifier = identifier.into_string().map_err(|_|"检查应用标识编码无效")?;
    }
    context.config_mut().app.windows[0].url = tauri::WebviewUrl::App("studio".into());
    context.config_mut().app.windows[0].visible = false;
    context.config_mut().app.windows[0].focus = false;
    context.config_mut().app.windows[0].skip_taskbar = true;
    context.config_mut().app.windows[0].background_throttling =
        Some(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    let mut app = pixoma_studio::desktop_builder()
        .on_page_load(move |webview, payload| {
            eprintln!("页面加载事件：{:?} {}", payload.event(), payload.url());
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
                && !started.swap(true, Ordering::SeqCst)
            {
                webview.eval(&script).expect("启动页面布局检查失败");
            }
        })
        .build(context)?;
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Prohibited);
    app.listen("studio:layout-progress", |event| {
        eprintln!("页面检查进度：{}", event.payload());
    });
    let handle = app.handle().clone();
    app.listen("studio:layout-size", move |event| {
        let size: [f64; 2] = serde_json::from_str(event.payload()).expect("窗口尺寸格式无效");
        handle
            .get_webview_window("main")
            .expect("Studio 主窗口不存在")
            .set_size(tauri::LogicalSize::new(size[0], size[1]))
            .expect("调整窗口尺寸失败");
    });
    let handle = app.handle().clone();
    app.listen("studio:layout-show", move |_| {
        handle.get_webview_window("main").expect("Studio 主窗口不存在").show().expect("显示检查窗口失败");
    });
    let handle = app.handle().clone();
    app.listen("studio:layout-focus", move |_| {
        #[cfg(target_os = "macos")]
        {
            handle.set_activation_policy(tauri::ActivationPolicy::Accessory).expect("启用检查窗口交互失败");
            handle.show().expect("显示检查应用失败");
        }
        let window = handle.get_webview_window("main").expect("Studio 主窗口不存在");
        window.unminimize().expect("恢复检查窗口失败");
        window.show().expect("显示检查窗口失败");
        window.set_focus().expect("激活检查窗口失败");
    });
    let handle = app.handle().clone();
    let completion = finished.clone();
    let result = succeeded.clone();
    app.listen("studio:layout-result", move |event| {
        let payload: serde_json::Value =
            serde_json::from_str(event.payload()).expect("布局检查结果格式无效");
        let success = payload["success"] == true;
        println!("{payload}");
        completion.store(true, Ordering::SeqCst);
        result.store(success, Ordering::SeqCst);
        handle.exit(if success { 0 } else { 1 });
    });
    let handle = app.handle().clone();
    let completion = finished.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(timeout_seconds));
        if !completion.load(Ordering::SeqCst) {
            eprintln!("页面布局检查超过 {timeout_seconds} 秒");
            handle.exit(1);
        }
    });
    let exit_code = app.run_return(|_, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit) {
            eprintln!("页面检查应用事件：{event:?}");
        }
    });
    eprintln!("页面检查应用结束：{exit_code}");
    if !finished.load(Ordering::SeqCst) || !succeeded.load(Ordering::SeqCst) {
        return Err("原生页面布局检查失败".into());
    }
    Ok(())
}
