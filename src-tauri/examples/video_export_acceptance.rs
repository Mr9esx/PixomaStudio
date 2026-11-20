use pixoma_studio::Store;
use serde_json::{Value,json};
use std::{path::PathBuf,process::Command};

#[tokio::main]
async fn main() -> Result<(),Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要验收数据目录")?);
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let manifest: Value = serde_json::from_slice(&std::fs::read(root.join("video-acceptance.json"))?)?;
    let session = manifest["session_id"].as_str().ok_or("会话编号缺失")?;
    let assets = store.list_session_assets(session).await?;
    let asset = assets.iter().filter(|asset|asset.versions.iter().any(|version|version.content_origin == "video_export")).max_by_key(|asset|&asset.updated_at).ok_or("导出成片缺失")?;
    let version = asset.versions.iter().find(|version|version.version == asset.current_version).ok_or("成片版本缺失")?;
    let path = root.join("山间茶事-15s.mp4");
    std::fs::write(&path,store.read_asset(&asset.id,&version.id).await?)?;
    let probe = Command::new(env!("PIXOMA_FFPROBE_PATH")).args(["-v","error","-count_frames","-show_streams","-show_format","-of","json"]).arg(&path).output()?;
    if !probe.status.success() {return Err(String::from_utf8_lossy(&probe.stderr).into_owned().into());}
    let probe: Value = serde_json::from_slice(&probe.stdout)?;
    let streams = probe["streams"].as_array().ok_or("媒体流缺失")?;
    let video = streams.iter().find(|stream|stream["codec_type"] == "video").ok_or("视频流缺失")?;
    assert_eq!(video["codec_name"],"h264");
    assert_eq!(video["width"],1280);
    assert_eq!(video["height"],720);
    assert_eq!(video["avg_frame_rate"],"30/1");
    assert_eq!(video["nb_read_frames"],"450");
    let video_duration: f64 = video["duration"].as_str().ok_or("视频时长缺失")?.parse()?;
    assert!((video_duration-15.0).abs() < 0.001,"视频时长需要十五秒，实际{video_duration}");
    assert!(streams.iter().any(|stream|stream["codec_type"] == "audio" && stream["codec_name"] == "aac"));
    let decoded = Command::new("ffmpeg").args(["-v","error","-nostdin","-i"]).arg(&path).args(["-f","null","-"]).output()?;
    assert!(decoded.status.success(),"完整解码失败：{}",String::from_utf8_lossy(&decoded.stderr));
    let project = store.read_video_project(session).await?;
    let shots = project["project"]["shots"].as_array().ok_or("分镜缺失")?;
    assert_eq!(shots.len(),5);
    assert!(shots.iter().all(|shot|shot["media"]["firstFrame"]["assetId"].is_string() && shot["media"]["lastFrame"]["assetId"].is_string()));
    let receipt = json!({"success":true,"file":path,"session_id":session,"asset_id":asset.id,"asset_version_id":version.id,"video_duration":video_duration,"frames":450,"fps":30,"width":1280,"height":720,"codec":"h264","audio_codec":"aac","decoded":true,"shots":5,"boundary_frames":10,"probe":probe});
    std::fs::write(root.join("final-video-verification.json"),serde_json::to_vec_pretty(&receipt)?)?;
    println!("{}",serde_json::to_string_pretty(&receipt)?);
    store.close().await;
    Ok(())
}
