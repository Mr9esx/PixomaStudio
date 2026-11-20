use crate::{Error, Result, Store, assets::{AssetContent, AssetCreation}, asset_analysis::media_binary};
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportShot {
    asset_id: String,
    asset_version_id: String,
    source_in: f64,
    duration: f64,
    #[serde(default)]
    subtitle: String,
    #[serde(default = "default_volume")]
    volume: f64,
}

fn default_volume() -> f64 { 1.0 }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportAudio {
    asset_id: String,
    asset_version_id: String,
    start: f64,
    duration: f64,
    source_in: f64,
    volume: f64,
    #[serde(default)]
    r#loop: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VideoExport {
    session_id: String,
    request_id: String,
    name: String,
    width: u32,
    height: u32,
    fps: u32,
    shots: Vec<ExportShot>,
    #[serde(default)]
    audio: Vec<ExportAudio>,
}

async fn checked(command: &mut Command) -> Result<Vec<u8>> {
    let output = command.output().await?;
    if !output.status.success() {
        return Err(Error::Validation(format!("视频处理失败：{}", String::from_utf8_lossy(&output.stderr))));
    }
    Ok(output.stdout)
}

impl Store {
    pub async fn video_export_status(&self, session_id: &str, request_id: &str) -> Result<Value> {
        self.get_session(session_id).await?;
        crate::error::required(request_id, "导出请求标识不能为空")?;
        let active_session = self.video_exports.lock().await.get(request_id)
            .filter(|(_, request)| request.strong_count() > 0).map(|(session, _)| session.clone());
        if active_session.as_deref().is_some_and(|session| session != session_id) { return Err(Error::NotFound); }
        let asset_id: Option<String> = sqlx::query_scalar("SELECT asset_id FROM asset_operations WHERE request_id=?")
            .bind(request_id).fetch_optional(&self.pool).await?;
        if let Some(asset_id) = asset_id {
            let asset = self.get_asset(&asset_id).await?;
            if !asset.usages.iter().any(|usage| usage["session_id"] == session_id) { return Err(Error::NotFound); }
            if let Some(version) = asset.versions.iter().find(|version| version.metadata["video_export_request"].is_string()) {
                return Ok(serde_json::json!({"status":"completed","result":{"assetId":asset.id,"versionId":version.id}}));
            }
            if active_session.is_none() { return Err(Error::Conflict("请求标识未用于视频导出".to_owned())); }
        }
        Ok(serde_json::json!({"status":if active_session.is_some() { "exporting" } else { "interrupted" }}))
    }

    pub async fn export_video_project(&self, input: Value) -> Result<Value> {
        let session = crate::local_api::text(&input,"session_id")?.to_owned();
        self.export_video_project_owned(input,&session).await
    }

    pub(crate) async fn export_video_project_owned(&self, input: Value, owner_session: &str) -> Result<Value> {
        let fingerprint = crate::assets::digest(&serde_json::to_vec(&input)?);
        let input: VideoExport = serde_json::from_value(input)?;
        crate::error::required(&input.session_id,"导出来源对话标识不能为空")?;
        self.get_session(owner_session).await?;
        let request_lock = {
            let mut requests = self.video_exports.lock().await;
            requests.retain(|_, (_, request)| request.strong_count() > 0);
            let (session, request) = requests.get(&input.request_id).and_then(|(session, request)| request.upgrade().map(|request| (session.clone(), request)))
                .unwrap_or_else(|| (owner_session.to_owned(), std::sync::Arc::new(tokio::sync::Mutex::new(()))));
            requests.insert(input.request_id.clone(), (session, std::sync::Arc::downgrade(&request)));
            request
        };
        let _request_guard = request_lock.lock().await;
        let previous: Option<String> = sqlx::query_scalar("SELECT asset_id FROM asset_operations WHERE request_id=?")
            .bind(&input.request_id).fetch_optional(&self.pool).await?;
        if let Some(id) = previous {
            let asset = self.get_asset(&id).await?;
            if asset.versions.iter().any(|version|version.metadata["video_export_request"] == fingerprint) {return Ok(serde_json::to_value(asset)?);}
            return Err(Error::Conflict("请求标识已用于其他导出参数".to_owned()));
        }
        if input.shots.is_empty() || input.shots.len() > 100 || input.audio.len() > 100 || input.width < 16 || input.height < 16 || input.width > 4096 || input.height > 4096 || input.width % 2 != 0 || input.height % 2 != 0 || input.fps == 0 || input.fps > 60 {
            return Err(Error::Invalid("视频尺寸、帧率或分镜数量无效"));
        }
        let directory = self.root.join("video-exports").join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&directory)?;
        let mut files = Vec::new();
        let mut total = 0.0;
        for (index, shot) in input.shots.iter().enumerate() {
            if !shot.source_in.is_finite() || shot.source_in < 0.0 || !shot.duration.is_finite() || shot.duration <= 0.0 || !shot.volume.is_finite() || !(0.0..=2.0).contains(&shot.volume) || shot.subtitle.len() > 4096 {
                return Err(Error::Invalid("分镜剪辑范围无效"));
            }
            if (shot.duration * f64::from(input.fps) - (shot.duration * f64::from(input.fps)).round()).abs() > 0.000001 {return Err(Error::Invalid("分镜时长需要按项目帧率设置"));}
            total += shot.duration;
            if total > 3600.0 { return Err(Error::Invalid("视频时长超过一小时")); }
            let source = self.asset_path(&shot.asset_id, &shot.asset_version_id).await?;
            let probe = checked(Command::new(media_binary("ffprobe")?).args(["-v", "error", "-show_streams", "-show_format", "-of", "json"]).arg(&source)).await?;
            let probe: Value = serde_json::from_slice(&probe)?;
            let streams = probe["streams"].as_array().ok_or(Error::Invalid("视频媒体信息缺失"))?;
            if !streams.iter().any(|stream| stream["codec_type"] == "video") { return Err(Error::Invalid("分镜素材需要视频")); }
            let duration: f64 = probe["format"]["duration"].as_str().ok_or(Error::Invalid("源视频时长缺失"))?.parse().map_err(|_| Error::Invalid("源视频时长无效"))?;
            if shot.source_in + shot.duration > duration + 1.0 / f64::from(input.fps) { return Err(Error::Invalid("剪辑范围超出源视频")); }
            let audio = streams.iter().any(|stream| stream["codec_type"] == "audio");
            let destination = directory.join(format!("clip-{index}.mp4"));
            let mut filter = format!("scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,fps={},setsar=1", input.width, input.height, input.width, input.height, input.fps);
            if !shot.subtitle.trim().is_empty() {
                #[cfg(target_os = "macos")]
                let font = "/System/Library/Fonts/STHeiti Medium.ttc";
                #[cfg(target_os = "windows")]
                let font = "C:/Windows/Fonts/msyh.ttc";
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                let font = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc";
                std::fs::copy(font,directory.join("font.ttc"))?;
                std::fs::write(directory.join(format!("subtitle-{index}.txt")),&shot.subtitle)?;
                filter.push_str(&format!(",drawtext=fontfile=font.ttc:textfile=subtitle-{index}.txt:expansion=none:fontcolor=white:fontsize={}:borderw=2:bordercolor=black@0.65:x=(w-tw)/2:y=h-th-{}",input.height / 25,input.height / 15));
            }
            let mut command = Command::new(media_binary("ffmpeg")?);
            command.current_dir(&directory);
            command.args(["-v", "error", "-nostdin", "-ss", &shot.source_in.to_string(), "-i"]).arg(&source);
            if audio {command.args(["-af",&format!("volume={},apad",shot.volume)]);} else { command.args(["-filter_complex",&format!("anullsrc=r=48000:cl=stereo,volume={}[silence]",shot.volume)]); }
            command.args(["-t", &shot.duration.to_string(), "-frames:v", &(shot.duration * f64::from(input.fps)).round().to_string(), "-map", "0:v:0", "-map", if audio { "0:a:0" } else { "[silence]" }, "-vf", &filter, "-c:v", if cfg!(target_os="macos") {"h264_videotoolbox"} else {"libopenh264"}, "-pix_fmt", "yuv420p", "-b:v", "6000000", "-c:a", "aac", "-ar", "48000", "-ac", "2", "-y"]);
            #[cfg(target_os="macos")]
            command.args(["-allow_sw","1"]);
            command.arg(&destination);
            checked(&mut command).await?;
            files.push(format!("file 'clip-{index}.mp4'\nduration {}",shot.duration));
        }
        let list = directory.join("clips.txt");
        std::fs::write(&list, files.join("\n"))?;
        let merged = directory.join("merged.mp4");
        checked(Command::new(media_binary("ffmpeg")?).args(["-v", "error", "-nostdin", "-f", "concat", "-safe", "1", "-i"]).arg(&list).args(["-t",&total.to_string(),"-c", "copy", "-movflags", "+faststart", "-y"]).arg(&merged)).await?;
        let destination = if input.audio.is_empty() { merged } else {
            let destination = directory.join("final.mp4");
            let mut command = Command::new(media_binary("ffmpeg")?);
            command.args(["-v","error","-nostdin","-i"]).arg(&merged);
            let mut filters = vec![format!("[0:a]atrim=duration={total},asetpts=PTS-STARTPTS[base]")];
            let mut labels = "[base]".to_owned();
            for (index,clip) in input.audio.iter().enumerate() {
                if ![clip.start,clip.duration,clip.source_in,clip.volume].iter().all(|value|value.is_finite()) || clip.start < 0.0 || clip.start >= total || clip.duration <= 0.0 || clip.source_in < 0.0 || !(0.0..=2.0).contains(&clip.volume) { return Err(Error::Invalid("音频剪辑范围或音量无效")); }
                let source = self.asset_path(&clip.asset_id,&clip.asset_version_id).await?;
                let probe = checked(Command::new(media_binary("ffprobe")?).args(["-v","error","-show_streams","-show_format","-of","json"]).arg(&source)).await?;
                let probe: Value = serde_json::from_slice(&probe)?;
                if !probe["streams"].as_array().is_some_and(|streams|streams.iter().any(|stream|stream["codec_type"] == "audio")) { return Err(Error::Invalid("所选素材没有音频")); }
                let duration: f64 = probe["format"]["duration"].as_str().ok_or(Error::Invalid("音频时长缺失"))?.parse().map_err(|_|Error::Invalid("音频时长无效"))?;
                if clip.source_in >= duration || (!clip.r#loop && clip.source_in + clip.duration > duration + 0.05) { return Err(Error::Invalid("音频剪辑范围超出素材")); }
                if clip.r#loop && clip.duration > duration - clip.source_in {
                    let trimmed = directory.join(format!("audio-loop-{index}.wav"));
                    checked(Command::new(media_binary("ffmpeg")?).args(["-v","error","-nostdin","-ss",&clip.source_in.to_string(),"-i"]).arg(&source).args(["-vn","-c:a","pcm_s16le","-ar","48000","-ac","2","-y"]).arg(&trimmed)).await?;
                    command.args(["-stream_loop","-1","-i"]).arg(trimmed);
                } else {
                    command.args(["-ss",&clip.source_in.to_string(),"-i"]).arg(source);
                }
                let input_index = index + 1;
                filters.push(format!("[{input_index}:a]atrim=duration={},asetpts=PTS-STARTPTS,volume={},adelay={}:all=1[a{index}]",clip.duration,clip.volume,(clip.start * 1000.0).round() as u64));
                labels.push_str(&format!("[a{index}]"));
            }
            filters.push(format!("{labels}amix=inputs={}:duration=first:normalize=0,alimiter=limit=0.95[mix]",input.audio.len()+1));
            command.args(["-filter_complex",&filters.join(";"),"-map","0:v:0","-map","[mix]","-t",&total.to_string(),"-c:v","copy","-c:a","aac","-ar","48000","-ac","2","-movflags","+faststart","-y"]).arg(&destination);
            checked(&mut command).await?;
            destination
        };
        let (asset, _) = self.create_asset_details(AssetCreation { session_id: Some(owner_session), project_id: None, name: &input.name, content: AssetContent::File(&destination), origin: "user", content_origin: "video_export", request_id: &input.request_id, source_modified_at: None }).await?;
        sqlx::query("UPDATE asset_versions SET metadata=json_set(metadata,'$.video_export_request',?) WHERE asset_id=?")
            .bind(fingerprint).bind(&asset.id).execute(&self.pool).await?;
        std::fs::remove_dir_all(directory)?;
        Ok(serde_json::to_value(self.get_asset(&asset.id).await?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn file_registration_rejects_invalid_video_without_creating_assets() -> Result<()> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new().prefix("video-export-invalid-").tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let session = store.create_session("invalid-file-export").await?;
        let source = directory.path().join("invalid.mp4");
        std::fs::write(&source, b"invalid video")?;
        let invalid = store.create_asset_details(AssetCreation { session_id: Some(&session.id), project_id: None, name: "无效视频.mp4", content: AssetContent::File(&source), origin: "user", content_origin: "video_export", request_id: "invalid-export-file", source_modified_at: None }).await;
        assert!(invalid.is_err());
        assert!(store.list_session_assets(&session.id).await?.is_empty());
        assert_eq!(store.library_assets(&Default::default()).await?["total"], 0);
        store.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn file_backed_exports_register_and_save_large_video() -> Result<()> {
        use std::io::Read;
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new().prefix("video-export-large-").tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let session = store.create_session("large-export").await?;
        let source = directory.path().join("source.mp4");
        std::fs::write(&source, include_bytes!("../tests/fixtures/red.mp4"))?;
        let rendered = directory.path().join("rendered.mov");
        checked(Command::new(media_binary("ffmpeg")?).args(["-v","error","-nostdin","-stream_loop","-1","-i"]).arg(&source)
            .args(["-filter_complex","anullsrc=r=48000:cl=stereo[silence]","-map","0:v:0","-map","[silence]","-c:v","copy","-c:a","pcm_s16le","-t","2900","-y"]).arg(&rendered)).await?;
        let size = std::fs::metadata(&rendered)?.len();
        assert!(size > 512 * 1024 * 1024);
        assert!(store.import_asset(&session.id, &rendered).await.is_err());
        let (asset, repeated) = store.create_asset_details(AssetCreation { session_id: Some(&session.id), project_id: None, name: "长片.mov", content: AssetContent::File(&rendered), origin: "user", content_origin: "video_export", request_id: "large-export", source_modified_at: None }).await?;
        assert!(!repeated);
        assert_eq!(asset.kind, "video");
        let version = &asset.versions[0];
        assert_eq!(version.size_bytes as u64, size);
        assert_eq!(version.width_px, Some(80));
        assert_eq!(version.height_px, Some(40));
        assert!((version.metadata["duration"].as_f64().expect("视频时长缺失") - 2900.0).abs() < 1.0);
        let saved = directory.path().join("保存的长片.mov");
        store.export_asset_version(&asset.id, &version.id).await?.save_to(&saved)?;
        let mut original = std::fs::File::open(&rendered)?;
        let mut exported = std::fs::File::open(&saved)?;
        let mut original_bytes = [0_u8; 65536];
        let mut exported_bytes = [0_u8; 65536];
        loop {
            let count = original.read(&mut original_bytes)?;
            if count == 0 { break; }
            exported.read_exact(&mut exported_bytes[..count])?;
            assert_eq!(original_bytes[..count], exported_bytes[..count]);
        }
        assert_eq!(exported.read(&mut exported_bytes)?, 0);
        std::fs::remove_file(rendered)?;
        assert_eq!(std::fs::metadata(store.asset_path(&asset.id, &version.id).await?)?.len(), size);
        store.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_retries_encode_one_result() -> Result<()> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new().prefix("video-export-concurrent-").tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let session = store.create_session("concurrent-export").await?;
        let source = directory.path().join("source.mp4");
        std::fs::write(&source, include_bytes!("../tests/fixtures/red.mp4"))?;
        let asset = store.import_asset(&session.id, &source).await?;
        let input = json!({"session_id":session.id,"request_id":"concurrent-export","name":"并发导出.mp4","width":320,"height":180,"fps":30,
            "shots":[{"asset_id":asset.id,"asset_version_id":asset.versions[0].id,"source_in":0,"duration":1}],"audio":[]});
        let api = crate::LocalApi::new(store.clone());
        let status_path = format!("/api/v1/studio/sessions/{}/video-exports/concurrent-export", session.id);
        let unknown = api.request(crate::LocalRequest { method: "GET".into(), path: status_path.clone(), body: None }).await?;
        assert_eq!(unknown["status"], "interrupted");
        let first_store = store.clone();
        let first_input = input.clone();
        let first = tokio::spawn(async move { first_store.export_video_project(first_input).await });
        let second_store = store.clone();
        let second_input = input.clone();
        let second = tokio::spawn(async move { second_store.export_video_project(second_input).await });
        let mut peak = 0;
        let mut running_checked = false;
        while !first.is_finished() || !second.is_finished() {
            let exports = directory.path().join("video-exports");
            if exports.exists() { peak = peak.max(std::fs::read_dir(exports)?.count()); }
            if peak > 0 && !running_checked {
                let status = api.request(crate::LocalRequest { method: "GET".into(), path: status_path.clone(), body: None }).await?;
                assert_eq!(status["status"], "exporting");
                running_checked = true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        let first = first.await.expect("首个导出任务应正常结束")?;
        let second = second.await.expect("重复导出任务应正常结束")?;
        assert_eq!(first["id"], second["id"]);
        assert!(running_checked);
        let completed = api.request(crate::LocalRequest { method: "GET".into(), path: status_path.clone(), body: None }).await?;
        assert_eq!(completed["status"], "completed");
        assert_eq!(completed["result"]["assetId"], first["id"]);
        assert_eq!(completed["result"]["versionId"], first["versions"][0]["id"]);
        let other_session = store.create_session("different-export-session").await?;
        let other_path = format!("/api/v1/studio/sessions/{}/video-exports/concurrent-export", other_session.id);
        assert!(api.request(crate::LocalRequest { method: "GET".into(), path: other_path, body: None }).await.is_err());
        assert_eq!(peak, 1, "同一请求同时创建了多个编码任务");
        let result = store.asset_path(first["id"].as_str().expect("导出资产缺失"), first["versions"][0]["id"].as_str().expect("导出版本缺失")).await?;
        let saved = directory.path().join("保存的成片.mp4");
        store.export_asset_version(first["id"].as_str().expect("导出资产缺失"), first["versions"][0]["id"].as_str().expect("导出版本缺失")).await?.save_to(&saved)?;
        assert_eq!(std::fs::read(&saved)?, std::fs::read(result)?);
        let decoded = directory.path().join("decoded");
        std::fs::create_dir(&decoded)?;
        checked(Command::new(media_binary("ffmpeg")?).args(["-v","error","-xerror","-nostdin","-i"]).arg(saved).args(["-map","0:v:0","-an","-fps_mode","passthrough"]).arg(decoded.join("frame-%03d.png"))).await?;
        assert_eq!(std::fs::read_dir(decoded)?.count(), 30);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM asset_operations WHERE request_id='concurrent-export'").fetch_one(&store.pool).await?;
        assert_eq!(count, 1);
        let mut conflict = input.clone();
        conflict["name"] = json!("其它导出参数.mp4");
        assert!(matches!(store.export_video_project(conflict).await, Err(Error::Conflict(_))));
        store.close().await;
        let reopened = Store::open(&directory.path().join("studio.sqlite")).await?;
        let recovered_status = crate::LocalApi::new(reopened.clone()).request(crate::LocalRequest { method: "GET".into(), path: status_path, body: None }).await?;
        assert_eq!(recovered_status, completed);
        let restored = reopened.export_video_project(input).await?;
        assert_eq!(restored["id"], first["id"]);
        assert_eq!(std::fs::read_dir(directory.path().join("video-exports"))?.count(), 0);
        reopened.close().await;
        Ok(())
    }
}
