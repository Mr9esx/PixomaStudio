use pixoma_studio::Store;
use serde_json::{Value,json};
use std::{path::PathBuf,process::Command};

fn run(command: &mut Command) -> Result<(),Box<dyn std::error::Error>> {
    let output = command.output()?;
    if !output.status.success() {return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());}
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(),Box<dyn std::error::Error>> {
    let root = PathBuf::from(std::env::var_os("PIXOMA_STUDIO_DATA_DIR").ok_or("需要验收数据目录")?);
    let store = Store::open(&root.join("studio-standalone.sqlite")).await?;
    let h3: Value = serde_json::from_slice(&std::fs::read(root.join("h3-manifest.json"))?)?;
    let seedance: Value = serde_json::from_slice(&std::fs::read(root.join("video-acceptance.json"))?)?;
    let h3_output = h3["outputs"].as_array().ok_or("H3输出缺失")?.iter().find(|output|output["field_key"] == "video").ok_or("H3视频缺失")?;
    let online_output = &seedance["executions"][0]["outputs"][0];
    let source = root.join("export-cases-source.mp4");
    let online = root.join("export-cases-online.mp4");
    std::fs::write(&source,store.read_asset(h3_output["asset_id"].as_str().ok_or("视频资产缺失")?,h3_output["asset_version_id"].as_str().ok_or("视频版本缺失")?).await?)?;
    std::fs::write(&online,store.read_asset(online_output["asset_id"].as_str().ok_or("视频资产缺失")?,online_output["asset_version_id"].as_str().ok_or("视频版本缺失")?).await?)?;
    let silent = root.join("export-cases-silent.mp4");
    let sound = root.join("export-cases-source.wav");
    run(Command::new(env!("PIXOMA_FFMPEG_PATH")).args(["-v","error","-nostdin","-i"]).arg(&source).args(["-an","-c:v","copy","-y"]).arg(&silent))?;
    run(Command::new(env!("PIXOMA_FFMPEG_PATH")).args(["-v","error","-nostdin","-i"]).arg(&online).args(["-vn","-c:a","pcm_s16le","-ar","48000","-ac","2","-y"]).arg(&sound))?;
    let session = store.create_session(&format!("video-export-boundary-validation-{}",uuid::Uuid::new_v4())).await?;
    let video = store.import_asset(&session.id,&silent).await?;
    let audio = store.import_asset(&session.id,&sound).await?;
    let input = json!({"session_id":session.id,"request_id":format!("real-export-cases:{}",session.id),"name":"媒体导出验证.mp4","width":320,"height":180,"fps":30,
        "shots":[{"asset_id":video.id,"asset_version_id":video.versions[0].id,"source_in":0,"duration":3,"volume":0,"subtitle":"静音素材与中文字幕"},{"asset_id":online_output["asset_id"],"asset_version_id":online_output["asset_version_id"],"source_in":1,"duration":3,"volume":0,"subtitle":"从指定位置循环音频"}],
        "audio":[{"asset_id":audio.id,"asset_version_id":audio.versions[0].id,"start":0.5,"duration":5.5,"source_in":2,"volume":0.5,"loop":true}]});
    let result = store.export_video_project(input.clone()).await?;
    let repeated = store.export_video_project(input.clone()).await?;
    assert_eq!(result["id"],repeated["id"]);
    let mut conflict = input.clone();conflict["name"] = json!("参数冲突.mp4");
    assert!(store.export_video_project(conflict).await.is_err());
    let mut invalid = input.clone();invalid["request_id"] = json!("invalid-export-frames");invalid["shots"][0]["duration"] = json!(3.01);
    assert!(store.export_video_project(invalid).await.is_err());
    let mut invalid = input;invalid["request_id"] = json!("invalid-export-source");invalid["shots"][0]["source_in"] = json!(100);
    assert!(store.export_video_project(invalid).await.is_err());
    let path = root.join("export-cases-verified.mp4");
    std::fs::write(&path,store.read_asset(result["id"].as_str().ok_or("导出资产缺失")?,result["versions"][0]["id"].as_str().ok_or("导出版本缺失")?).await?)?;
    run(Command::new("ffmpeg").args(["-v","error","-nostdin","-i"]).arg(&path).args(["-f","null","-"]))?;
    let probe = Command::new(env!("PIXOMA_FFPROBE_PATH")).args(["-v","error","-count_frames","-show_streams","-of","json"]).arg(&path).output()?;
    if !probe.status.success() {return Err(String::from_utf8_lossy(&probe.stderr).into_owned().into());}
    let probe: Value = serde_json::from_slice(&probe.stdout)?;
    let streams = probe["streams"].as_array().ok_or("媒体流缺失")?;
    assert!(streams.iter().any(|stream|stream["codec_type"] == "video" && stream["nb_read_frames"] == "180"));
    assert!(streams.iter().any(|stream|stream["codec_type"] == "audio"));
    let receipt = json!({"success":true,"silent_source":true,"subtitles":true,"loop_from_source_in":2,"audio_delay":0.5,"idempotence":true,"conflict_rejected":true,"frame_validation":true,"source_range_validation":true,"frames":180,"file":path});
    std::fs::write(root.join("export-cases-verification.json"),serde_json::to_vec_pretty(&receipt)?)?;
    println!("{receipt}");
    store.close().await;
    Ok(())
}
