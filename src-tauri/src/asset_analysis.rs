use crate::{Error, Result, Store, error::timestamp};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

const VIDEO_ANALYSIS_PREFIX: &str = ".pixoma-video-analysis-";

pub(crate) fn media_binary(name: &str) -> Result<std::path::PathBuf> {
    if !matches!(name, "ffmpeg" | "ffprobe") {
        return Err(Error::Invalid("媒体程序名称无效"));
    }
    #[cfg(debug_assertions)]
    {
        let path = match name {
            "ffmpeg" => env!("PIXOMA_FFMPEG_PATH"),
            "ffprobe" => option_env!("PIXOMA_FFPROBE_PATH").ok_or(Error::State("ffprobe 路径未配置"))?,
            _ => unreachable!(),
        };
        Ok(std::path::PathBuf::from(path))
    }
    #[cfg(not(debug_assertions))]
    {
        Ok(std::env::current_exe()?.parent().ok_or(Error::State("应用程序目录不存在"))?
            .join(if cfg!(target_os = "windows") { format!("{name}.exe") } else { name.to_owned() }))
    }
}

pub(crate) fn remove_interrupted_video_analysis(root: &std::path::Path) -> Result<()> {
    if !root.try_exists()? {
        return Ok(());
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(VIDEO_ANALYSIS_PREFIX))
        {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

pub(crate) fn analyze_image(bytes: &[u8]) -> Result<(u32, u32, Value)> {
    let image = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()?
        .decode()
        .map_err(|_| Error::Invalid("图片格式无效"))?;
    let dimensions = (image.width(), image.height());
    let sample = image
        .thumbnail(image.width().min(256), image.height().min(256))
        .to_rgba8();
    let pixels: Vec<exoquant::Color> = sample
        .pixels()
        .filter(|pixel| pixel[3] > 0)
        .map(|pixel| exoquant::Color::new(pixel[0], pixel[1], pixel[2], pixel[3]))
        .collect();
    let mut weights = BTreeMap::<String, u64>::new();
    if !pixels.is_empty() {
        let (palette, indices) = exoquant::convert_to_indexed(
            &pixels,
            sample.width() as usize,
            5,
            &exoquant::optimizer::KMeans,
            &exoquant::ditherer::None,
        );
        for (pixel, index) in pixels.iter().zip(indices) {
            let color = palette
                .get(usize::from(index))
                .ok_or(Error::State("调色盘颜色索引无效"))?;
            let hex = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
            *weights.entry(hex).or_default() += u64::from(pixel.a);
        }
    }
    let total: u64 = weights.values().sum();
    let mut colors: Vec<_> = weights.into_iter().collect();
    colors.sort_by(|first, second| second.1.cmp(&first.1).then_with(|| first.0.cmp(&second.0)));
    let colors: Vec<Value> = colors
        .into_iter()
        .map(|(hex, weight)| json!({ "hex": hex, "ratio": weight as f64 / total as f64 }))
        .collect();
    Ok((
        dimensions.0,
        dimensions.1,
        json!({ "status": "ready", "colors": colors, "analyzed_at": timestamp()? }),
    ))
}

impl Store {
    pub(crate) async fn probe_audio_metadata(&self, bytes: &[u8]) -> Result<Value> {
        let directory = tempfile::Builder::new().prefix(VIDEO_ANALYSIS_PREFIX).tempdir_in(&self.root)?;
        let input = directory.path().join("input.bin");
        tokio::fs::write(&input, bytes).await?;
        let mut command = tokio::process::Command::new(media_binary("ffprobe")?);
        command.args(["-v", "error", "-protocol_whitelist", "file,pipe", "-select_streams", "a:0",
            "-show_entries", "stream=codec_type:format=duration", "-of", "json"]).arg(input).kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(30), command.output()).await
            .map_err(|_| Error::Invalid("音频媒体信息读取超时"))??;
        if !output.status.success() { return Err(Error::Invalid("音频媒体信息无法读取")); }
        let document: Value = serde_json::from_slice(&output.stdout)?;
        if document["streams"][0]["codec_type"] != "audio" { return Err(Error::Invalid("素材没有音频内容")); }
        let duration: f64 = document["format"]["duration"].as_str().ok_or(Error::Invalid("音频时长缺失"))?
            .parse().map_err(|_| Error::Invalid("音频时长无效"))?;
        if !duration.is_finite() || duration <= 0.0 { return Err(Error::Invalid("音频时长无效")); }
        Ok(json!({"duration":duration}))
    }

    async fn probe_video_metadata(&self, input: &std::path::Path) -> Result<Value> {
        let mut command = tokio::process::Command::new(media_binary("ffprobe")?);
        command.args(["-v", "error", "-protocol_whitelist", "file,pipe", "-select_streams", "v:0",
            "-show_entries", "stream=duration,avg_frame_rate", "-of", "json"]).arg(input).kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(30), command.output()).await
            .map_err(|_| Error::Invalid("视频媒体信息读取超时"))??;
        if !output.status.success() { return Err(Error::Invalid("视频媒体信息无法读取")); }
        let document: Value = serde_json::from_slice(&output.stdout)?;
        let stream = &document["streams"][0];
        let duration: f64 = stream["duration"].as_str().ok_or(Error::Invalid("视频时长缺失"))?
            .parse().map_err(|_| Error::Invalid("视频时长无效"))?;
        let rate = stream["avg_frame_rate"].as_str().ok_or(Error::Invalid("视频帧率缺失"))?;
        let (numerator, denominator) = rate.split_once('/').ok_or(Error::Invalid("视频帧率格式无效"))?;
        let numerator: f64 = numerator.parse().map_err(|_| Error::Invalid("视频帧率无效"))?;
        let denominator: f64 = denominator.parse().map_err(|_| Error::Invalid("视频帧率无效"))?;
        let fps = numerator / denominator;
        if !duration.is_finite() || duration <= 0.0 || !fps.is_finite() || fps <= 0.0 {
            return Err(Error::Invalid("视频时长或帧率无效"));
        }
        Ok(json!({"duration":duration,"fps":fps}))
    }

    pub async fn ensure_video_metadata(&self, asset_id: &str, version_id: &str) -> Result<Value> {
        let asset = self.get_asset(asset_id).await?;
        if asset.kind != "video" { return Err(Error::Invalid("媒体信息来源需要视频资产")); }
        let version = asset.versions.iter().find(|version|version.id == version_id).ok_or(Error::NotFound)?;
        if version.metadata["duration"].as_f64().is_some_and(|value|value > 0.0)
            && version.metadata["fps"].as_f64().is_some_and(|value|value > 0.0) {
            return Ok(version.metadata.clone());
        }
        let metadata = self.probe_video_metadata(&self.asset_path(asset_id,version_id).await?).await?;
        sqlx::query("UPDATE asset_versions SET metadata=json_set(metadata,'$.duration',json(?),'$.fps',json(?)) WHERE asset_id=? AND id=?")
            .bind(metadata["duration"].to_string()).bind(metadata["fps"].to_string()).bind(asset_id).bind(version_id)
            .execute(&self.pool).await?;
        Ok(metadata)
    }

    pub(crate) async fn extract_video_boundary_frames(&self, bytes: &[u8]) -> Result<[Vec<u8>; 2]> {
        let binary = media_binary("ffmpeg")?;
        let directory = tempfile::Builder::new()
            .prefix(VIDEO_ANALYSIS_PREFIX)
            .tempdir_in(&self.root)?;
        let input = directory.path().join("input.bin");
        tokio::fs::write(&input, bytes).await?;
        let mut frames = Vec::new();
        for role in ["first", "last"] {
            let output = directory.path().join(format!("{role}.png"));
            let mut command = tokio::process::Command::new(&binary);
            command.args(["-nostdin", "-hide_banner", "-loglevel", "error",
                "-protocol_whitelist", "file,pipe", "-format_whitelist",
                "mov,matroska,webm,avi,mpeg,mpegts,flv,ogg,asf"]);
            if role == "last" {
                command.args(["-sseof", "-1"]);
            }
            command.arg("-i").arg(&input).args(["-map", "0:v:0"]);
            if role == "last" {
                command.args(["-update", "1", "-y"]);
            } else {
                command.args(["-frames:v", "1"]);
            }
            command.args(["-an", "-sn", "-dn", "-c:v", "png",
                "-threads", "1"]).arg(&output).kill_on_drop(true);
            let status = tokio::time::timeout(Duration::from_secs(60), command.output())
                .await.map_err(|_| Error::Invalid("视频首尾帧提取超时"))??;
            if !status.status.success() {
                return Err(Error::Validation(format!("视频{role}无法解码：{}", String::from_utf8_lossy(&status.stderr))));
            }
            if tokio::fs::metadata(&output).await?.len() > 32 * 1024 * 1024 {
                return Err(Error::Invalid("视频首尾帧超过大小限制"));
            }
            frames.push(tokio::fs::read(output).await?);
        }
        frames.try_into().map_err(|_| Error::State("视频首尾帧数量无效"))
    }

    pub(crate) async fn analyze_video(&self, bytes: &[u8]) -> Result<(u32, u32, Value, Vec<u8>, Value)> {
        let directory = tempfile::Builder::new().prefix(VIDEO_ANALYSIS_PREFIX).tempdir_in(&self.root)?;
        let input = directory.path().join("input.bin");
        tokio::fs::write(&input, bytes).await?;
        self.analyze_video_file(&input).await
    }

    pub(crate) async fn analyze_video_file(&self, input: &std::path::Path) -> Result<(u32, u32, Value, Vec<u8>, Value)> {
        let metadata = self.probe_video_metadata(input).await?;
        let binary = media_binary("ffmpeg")?;
        let directory = tempfile::Builder::new()
            .prefix(VIDEO_ANALYSIS_PREFIX)
            .tempdir_in(&self.root)?;
        let output = directory.path().join("poster.png");
        let mut command = tokio::process::Command::new(binary);
        command
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-format_whitelist",
                "mov,matroska,webm,avi,mpeg,mpegts,flv,ogg,asf",
                "-i",
            ])
            .arg(input)
            .args([
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-an",
                "-sn",
                "-dn",
                "-c:v",
                "png",
                "-threads",
                "1",
                "-fs",
                "33554432",
            ])
            .arg(&output)
            .kill_on_drop(true);
        let status = tokio::time::timeout(Duration::from_secs(30), command.output())
            .await
            .map_err(|_| Error::Invalid("视频封面生成超时"))??;
        if !status.status.success() {
            return Err(Error::Invalid("视频内容无法解码"));
        }
        if tokio::fs::metadata(&output).await?.len() > 32 * 1024 * 1024 {
            return Err(Error::Invalid("视频封面超过大小限制"));
        }
        let frame = tokio::fs::read(output).await?;
        let (width, height, palette) = analyze_image(&frame)?;
        let image =
            image::load_from_memory(&frame).map_err(|_| Error::Invalid("视频封面格式无效"))?;
        let mut poster = std::io::Cursor::new(Vec::new());
        image
            .thumbnail(width.min(1280), height.min(1280))
            .write_to(&mut poster, image::ImageFormat::Png)
            .map_err(|_| Error::State("视频封面编码失败"))?;
        Ok((width, height, palette, poster.into_inner(), metadata))
    }

    pub async fn retry_asset_palette(&self, asset: &str, version: &str) -> Result<()> {
        let asset = self.get_asset(asset).await?;
        let bytes = match asset.kind.as_str() {
            "image" => self.read_asset(&asset.id, version).await?,
            "video" => {
                self.asset_poster_response(&asset.id, version, "GET")
                    .await?
                    .body
            }
            _ => return Err(Error::Invalid("当前资产不支持调色盘分析")),
        };
        let (_, _, palette) = analyze_image(&bytes)?;
        let changed = sqlx::query("UPDATE asset_versions SET metadata=json_set(metadata,'$.palette',json(?)) WHERE asset_id=? AND id=?")
            .bind(palette.to_string()).bind(&asset.id).bind(version).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::NotFound);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn boundary_frames_decode_short_video_and_reject_invalid_media() -> Result<()> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new().prefix("video-frames-").tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let frames = store.extract_video_boundary_frames(include_bytes!("../tests/fixtures/red.mp4")).await?;
        for frame in frames {
            let image = image::load_from_memory(&frame).map_err(|_| Error::State("首尾帧图片格式无效"))?.to_rgb8();
            assert_eq!(image.dimensions(), (80, 40));
            let pixel = image.get_pixel(40, 20);
            assert!(pixel[0] > 240 && pixel[1] < 16 && pixel[2] < 16);
        }
        assert!(store.extract_video_boundary_frames(b"invalid video").await.is_err());
        store.close().await;
        Ok(())
    }

    #[test]
    fn startup_cleans_interrupted_video_files_inside_the_application_directory() -> Result<()> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("video-cleanup-")
            .tempdir_in(root)?;
        let interrupted = tempfile::Builder::new()
            .prefix(VIDEO_ANALYSIS_PREFIX)
            .tempdir_in(directory.path())?
            .keep();
        std::fs::write(
            interrupted.join("input.bin"),
            include_bytes!("../tests/fixtures/red.mp4"),
        )?;
        let retained = directory.path().join("retained");
        std::fs::create_dir(&retained)?;
        std::fs::write(retained.join("notes.txt"), "保留内容")?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            &retained,
            directory
                .path()
                .join(format!("{VIDEO_ANALYSIS_PREFIX}link")),
        )?;
        remove_interrupted_video_analysis(directory.path())?;
        assert!(!interrupted.exists());
        assert_eq!(
            std::fs::read_to_string(retained.join("notes.txt"))?,
            "保留内容"
        );
        Ok(())
    }
}
