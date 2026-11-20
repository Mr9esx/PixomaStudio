use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Listener, Manager};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要验收数据目录")?);
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join("video-acceptance.json"))?)?;
    let mode = if cfg!(dev) { "development" } else { "production" };
    let script = format!("window.__PIXOMA_VIDEO_SESSION__={}; window.__PIXOMA_TIMELINE_MODE__={}; {}", manifest["session_id"], serde_json::to_string(mode)?, include_str!("native_timeline.js"));
    let started = AtomicBool::new(false);
    let mut context = pixoma_studio::desktop_context();
    context.config_mut().identifier = "com.pixoma.studio.timeline-acceptance".into();
    context.config_mut().app.windows[0].background_throttling = Some(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    let app = pixoma_studio::desktop_builder().on_page_load(move |webview, payload| {
        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) && !started.swap(true, Ordering::SeqCst) {
            webview.eval(&script).expect("启动时间轴验收失败");
        }
    }).build(context)?;
    let handle = app.handle().clone();
    let output_root = root.clone();
    app.listen("studio:timeline-progress", move |event| {
        println!("{}", event.payload());
        let payload: serde_json::Value = serde_json::from_str(event.payload()).expect("验收记录格式无效");
        if payload["stage"] == "complete" || payload["stage"] == "failed" {
            std::fs::write(output_root.join("native-timeline-result.json"), serde_json::to_vec_pretty(&payload).expect("验收记录无法编码")).expect("验收记录无法保存");
            handle.exit(if payload["stage"] == "complete" { 0 } else { 1 });
        }
    });
    let exit_code = app.run_return(|app, event| {
        if matches!(event, tauri::RunEvent::Ready) {
            let window = app.get_webview_window("main").expect("Studio 主窗口不存在");
            window.show().expect("显示视频编辑器失败");
            window.set_focus().expect("激活视频编辑器失败");
        }
    });
    if exit_code != 0 { return Err("原生时间轴验收失败".into()); }
    let receipt_path = root.join("native-timeline-result.json");
    let mut receipt: serde_json::Value = serde_json::from_slice(&std::fs::read(&receipt_path)?)?;
    if receipt["success"] != true { return Err("原生时间轴验收失败".into()); }
    let path = root.join("native-timeline-export.mp4");
    let bytes = tauri::async_runtime::block_on(async {
        let store = pixoma_studio::Store::open(&root.join("studio-standalone.sqlite")).await?;
        let bytes = store.read_asset(receipt["export_asset_id"].as_str().ok_or(pixoma_studio::Error::State("导出资产缺失"))?, receipt["export_version_id"].as_str().ok_or(pixoma_studio::Error::State("导出版本缺失"))?).await?;
        store.close().await;
        Ok::<_, pixoma_studio::Error>(bytes)
    })?;
    std::fs::write(&path, bytes)?;
    let probe = std::process::Command::new(env!("PIXOMA_FFPROBE_PATH")).args(["-v", "error", "-count_frames", "-show_streams", "-of", "json"]).arg(&path).output()?;
    if !probe.status.success() { return Err(String::from_utf8_lossy(&probe.stderr).into_owned().into()); }
    let probe: serde_json::Value = serde_json::from_slice(&probe.stdout)?;
    let streams = probe["streams"].as_array().ok_or("导出媒体流缺失")?;
    assert!(streams.iter().any(|stream| stream["codec_type"] == "video" && stream["nb_read_frames"] == "450" && stream["width"] == 1280 && stream["height"] == 720 && stream["r_frame_rate"] == "30/1"));
    assert!(streams.iter().any(|stream| stream["codec_type"] == "audio"));
    let decoded = root.join("native-timeline-decoded.mp4");
    let decode = std::process::Command::new(env!("PIXOMA_FFMPEG_PATH")).args(["-v", "error", "-xerror", "-nostdin", "-i"]).arg(&path).args(["-c:v", "h264_videotoolbox", "-c:a", "aac", "-y"]).arg(&decoded).output()?;
    if !decode.status.success() { return Err(String::from_utf8_lossy(&decode.stderr).into_owned().into()); }
    receipt["media_verification"] = serde_json::json!({"file":path,"frames":450,"width":1280,"height":720,"fps":30,"audio":true,"full_decode":true});
    std::fs::write(receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
    println!("{}", serde_json::to_string(&receipt)?);
    Ok(())
}
