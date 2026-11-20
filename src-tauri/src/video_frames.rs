use crate::{Error, Result, Store, assets::{Asset, AssetContent, AssetCreation}, asset_analysis::media_binary};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameInput {
    session_id: String,
    frame: u64,
    fps: u32,
}

impl Store {
    pub async fn create_video_frame(&self, asset_id: &str, version_id: &str, input: Value) -> Result<Asset> {
        let input: FrameInput = serde_json::from_value(input)?;
        self.get_session(&input.session_id).await?;
        self.create_video_frame_owned(asset_id,version_id,input.frame,input.fps,Some(&input.session_id),&input.session_id).await
    }

    pub(crate) async fn create_video_frame_owned(&self, asset_id: &str, version_id: &str, frame: u64, fps: u32, session_id: Option<&str>, scope: &str) -> Result<Asset> {
        if !(1..=60).contains(&fps) {return Err(Error::Invalid("画面帧率需要为 1 至 60"));}
        let asset = self.get_asset(asset_id).await?;
        if asset.kind != "video" {return Err(Error::Invalid("画面来源需要视频资产"));}
        let metadata = self.ensure_video_metadata(asset_id,version_id).await?;
        let seconds = frame as f64 / f64::from(fps);
        if seconds >= metadata["duration"].as_f64().ok_or(Error::State("源视频时长缺失"))? {return Err(Error::Invalid("画面位置超出源视频"));}
        let request_id = format!("video-position:{scope}:{version_id}:{frame}:{fps}");
        let previous: Option<String> = sqlx::query_scalar("SELECT asset_id FROM asset_operations WHERE request_id=? AND operation='create'").bind(&request_id).fetch_optional(&self.pool).await?;
        if let Some(id) = previous {
            self.record_video_frame_source(&id,asset_id,version_id,frame,fps).await?;
            return self.get_asset(&id).await;
        }
        let source = self.asset_path(asset_id,version_id).await?;
        let directory = tempfile::Builder::new().prefix("video-position-").tempdir_in(&self.root)?;
        let destination = directory.path().join("frame.png");
        let mut command = tokio::process::Command::new(media_binary("ffmpeg")?);
        command.args(["-nostdin","-hide_banner","-loglevel","error","-protocol_whitelist","file,pipe","-i"])
            .arg(source).args(["-map","0:v:0","-vf",&format!("fps={fps},trim=start_frame={frame}"),"-frames:v","1","-an","-sn","-dn","-c:v","png","-threads","1"]).arg(&destination).kill_on_drop(true);
        let result = tokio::time::timeout(Duration::from_secs(60),command.output()).await.map_err(|_|Error::Invalid("视频画面提取超时"))??;
        if !result.status.success() {return Err(Error::Validation(format!("视频画面提取失败：{}",String::from_utf8_lossy(&result.stderr))));}
        if !destination.try_exists()? {return Err(Error::Invalid("指定位置没有可用的视频画面"));}
        if tokio::fs::metadata(&destination).await?.len()>32*1024*1024 {return Err(Error::Invalid("视频画面超过大小限制"));}
        let bytes = tokio::fs::read(destination).await?;
        let image = self.create_asset_details(AssetCreation {
            session_id,project_id:None,name:&format!("{} · 画面 {frame}.png",asset.name),
            content:AssetContent::Bytes(&bytes),origin:"workflow",content_origin:"derived",request_id:&request_id,source_modified_at:None,
        }).await?.0;
        self.record_video_frame_source(&image.id,asset_id,version_id,frame,fps).await?;
        self.get_asset(&image.id).await
    }

    async fn record_video_frame_source(&self, frame_asset_id: &str, asset_id: &str, version_id: &str, frame: u64, fps: u32) -> Result<()> {
        let source = json!({"asset_id":asset_id,"version_id":version_id,"frame":frame,"fps":fps});
        let result = sqlx::query("UPDATE asset_versions SET metadata=json_set(metadata,'$.source_video',json(?)) WHERE asset_id=? AND version=1")
            .bind(source.to_string()).bind(frame_asset_id).execute(&self.pool).await?;
        if result.rows_affected() != 1 {return Err(Error::State("视频画面初始版本不存在"));}
        Ok(())
    }
}
