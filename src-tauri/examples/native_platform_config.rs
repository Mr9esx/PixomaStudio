use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::Listener;

fn launch() -> Result<(), Box<dyn std::error::Error>> {
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let succeeded = Arc::new(AtomicBool::new(false));
    let app = pixoma_studio::desktop_builder()
        .on_page_load(move |webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
                && !started.swap(true, Ordering::SeqCst)
            {
                webview
                    .eval(include_str!("native_platform_bootstrap.js"))
                    .expect("启动平台配置检查失败");
            }
        })
        .build(pixoma_studio::desktop_context())?;
    let handle = app.handle().clone();
    let result = succeeded.clone();
    let completion = finished.clone();
    app.listen("studio:configuration-result", move |event| {
        let payload: serde_json::Value =
            serde_json::from_str(event.payload()).expect("平台配置检查结果无效");
        let success = payload["success"] == true;
        result.store(success, Ordering::SeqCst);
        completion.store(true, Ordering::SeqCst);
        println!("{payload}");
        handle.exit(if success { 0 } else { 1 });
    });
    let handle = app.handle().clone();
    let completion = finished.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(60));
        if !completion.load(Ordering::SeqCst) {
            eprintln!("平台配置检查超过 60 秒");
            handle.exit(1);
        }
    });
    app.run(|_, _| {});
    if !finished.load(Ordering::SeqCst) || !succeeded.load(Ordering::SeqCst) {
        return Err("原生平台配置检查失败".into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|argument| argument == "--launch") {
        return launch();
    }
    let address = std::env::var("PIXOMA_TEST_PLATFORM_URL")?;
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new()
        .prefix("native-platform-")
        .tempdir_in(root)?;
    let executable = std::env::current_exe()?;
    for with_environment in [true, false] {
        let mut command = std::process::Command::new(&executable);
        command
            .arg("--launch")
            .env("PIXOMA_STUDIO_DATA_DIR", directory.path())
            .env_remove("PIXOMA_PLATFORM_URL");
        if with_environment {
            command.env("PIXOMA_PLATFORM_URL", &address);
        }
        if !command.status()?.success() {
            return Err("原生窗口读取平台配置失败".into());
        }
        let configuration: serde_json::Value = serde_json::from_slice(&std::fs::read(
            directory.path().join("config/desktop.json"),
        )?)?;
        assert_eq!(configuration["platform_url"], address);
    }
    println!("原生窗口两次启动均已连接真实平台，第二次启动没有 PIXOMA_PLATFORM_URL");
    Ok(())
}
