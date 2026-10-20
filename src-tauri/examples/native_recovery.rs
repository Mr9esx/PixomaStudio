use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::Listener;

fn launch(phase: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var("PIXOMA_STUDIO_DATA_DIR")?);
    let state_file = root.join("recovery-check.json");
    let receipt_file = root.join(format!("recovery-{phase}.json"));
    let attempt = std::env::var("PIXOMA_TEST_RECOVERY_ATTEMPT")?;
    let mut input = serde_json::json!({ "phase": phase });
    match phase {
        "create" => {
            input["model"] = serde_json::from_str(&std::env::var("PIXOMA_TEST_MODEL_CONFIG")?)?;
            input["account"] = serde_json::json!({
                "username": std::env::var("PIXOMA_TEST_STUDIO_USERNAME")?,
                "password": std::env::var("PIXOMA_TEST_STUDIO_PASSWORD")?,
            });
        }
        "resume" => input["state"] = serde_json::from_slice(&std::fs::read(&state_file)?)?,
        _ => return Err("原生恢复检查阶段无效".into()),
    }
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let succeeded = Arc::new(AtomicBool::new(false));
    let script = format!(
        "window.__PIXOMA_RECOVERY_CHECK__ = {input}; {}",
        include_str!("native_recovery_bootstrap.js")
    );
    let app = pixoma_studio::desktop_builder()
        .on_page_load(move |webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
                && !started.swap(true, Ordering::SeqCst)
            {
                webview.eval(&script).expect("启动原生恢复检查失败");
            }
        })
        .build(pixoma_studio::desktop_context())?;
    app.listen("studio:recovery-progress", |event| {
        println!("{}", event.payload())
    });
    let handle = app.handle().clone();
    let result = succeeded.clone();
    let completion = finished.clone();
    let creating = phase == "create";
    let phase = phase.to_owned();
    app.listen("studio:recovery-result", move |event| {
        let payload: serde_json::Value =
            serde_json::from_str(event.payload()).expect("原生恢复结果格式无效");
        let success = payload["success"] == true;
        if success && creating {
            std::fs::write(
                &state_file,
                serde_json::to_vec(&payload).expect("保存检查记录格式失败"),
            )
            .expect("保存原生恢复检查记录失败");
        }
        let receipt = serde_json::json!({ "attempt": attempt, "phase": phase, "payload": payload });
        std::fs::write(
            &receipt_file,
            serde_json::to_vec(&receipt).expect("检查结果编码失败"),
        )
        .expect("保存原生恢复检查结果失败");
        result.store(success, Ordering::SeqCst);
        completion.store(true, Ordering::SeqCst);
        if creating {
            println!(
                "{}",
                serde_json::json!({ "success": success, "phase": "create", "state_saved": success })
            );
        } else {
            println!("{payload}");
        }
        handle.exit(if success { 0 } else { 1 });
    });
    let handle = app.handle().clone();
    let completion = finished.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(180));
        if !completion.load(Ordering::SeqCst) {
            eprintln!("原生恢复检查超过 180 秒");
            handle.exit(1);
        }
    });
    app.run(|_, _| {});
    if !finished.load(Ordering::SeqCst) || !succeeded.load(Ordering::SeqCst) {
        return Err("原生恢复检查失败".into());
    }
    Ok(())
}

fn phase_succeeded(
    root: &std::path::Path,
    phase: &str,
    attempt: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let bytes = match std::fs::read(root.join(format!("recovery-{phase}.json"))) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let receipt: serde_json::Value = serde_json::from_slice(&bytes)?;
    Ok(receipt["attempt"] == attempt
        && receipt["phase"] == phase
        && receipt["payload"]["success"] == true)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--launch") {
        return launch(&std::env::args().nth(2).ok_or("缺少原生检查阶段")?);
    }
    let executable = std::env::current_exe()?;
    let root = PathBuf::from(std::env::var("PIXOMA_STUDIO_DATA_DIR")?);
    for phase in ["create", "resume"] {
        let child = |attempt: &str| {
            let mut command = std::process::Command::new(&executable);
            command
                .arg("--launch")
                .arg(phase)
                .env("PIXOMA_TEST_RECOVERY_ATTEMPT", attempt);
            if phase == "resume" {
                command
                    .env_remove("PIXOMA_PLATFORM_URL")
                    .env_remove("PIXOMA_TEST_MODEL_CONFIG")
                    .env_remove("PIXOMA_TEST_STUDIO_USERNAME")
                    .env_remove("PIXOMA_TEST_STUDIO_PASSWORD");
            }
            command
        };
        if phase == "resume" {
            let lock = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(root.join(".application.lock"))?;
            lock.lock()?;
            let blocked_attempt = uuid::Uuid::new_v4().to_string();
            assert!(child(&blocked_attempt).status()?.success());
            assert!(
                !phase_succeeded(&root, phase, &blocked_attempt)?,
                "没有执行恢复检查的进程返回了成功结果"
            );
            drop(lock);
        }
        let attempt = uuid::Uuid::new_v4().to_string();
        if !child(&attempt).status()?.success() || !phase_succeeded(&root, phase, &attempt)? {
            return Err("原生窗口重新启动检查失败".into());
        }
        assert!(
            !phase_succeeded(&root, phase, &uuid::Uuid::new_v4().to_string())?,
            "其他启动编号读取了本次检查的成功结果"
        );
    }
    println!("原生窗口重新启动后的登录、项目、资料摘要和原审批运行检查通过");
    Ok(())
}
