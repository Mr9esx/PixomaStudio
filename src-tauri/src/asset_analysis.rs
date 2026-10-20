use crate::{Error, Result, Store, error::timestamp};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

const VIDEO_ANALYSIS_PREFIX: &str = ".pixoma-video-analysis-";

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
    pub(crate) async fn analyze_video(&self, bytes: &[u8]) -> Result<(u32, u32, Value, Vec<u8>)> {
        #[cfg(debug_assertions)]
        let binary = std::path::PathBuf::from(env!("PIXOMA_FFMPEG_PATH"));
        #[cfg(not(debug_assertions))]
        let binary = std::env::current_exe()?
            .parent()
            .ok_or(Error::State("应用程序目录不存在"))?
            .join(if cfg!(target_os = "windows") {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            });
        let directory = tempfile::Builder::new()
            .prefix(VIDEO_ANALYSIS_PREFIX)
            .tempdir_in(&self.root)?;
        let input = directory.path().join("input.bin");
        let output = directory.path().join("poster.png");
        tokio::fs::write(&input, bytes).await?;
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
        Ok((width, height, palette, poster.into_inner()))
    }

    pub async fn retry_asset_palette(
        &self,
        account: &str,
        asset: &str,
        version: &str,
    ) -> Result<()> {
        let asset = self.get_asset(account, asset).await?;
        let bytes = match asset.kind.as_str() {
            "image" => self.read_asset(account, &asset.id, version).await?,
            "video" => {
                self.asset_poster_response(account, &asset.id, version, "GET")
                    .await?
                    .body
            }
            _ => return Err(Error::Invalid("当前资产不支持调色盘分析")),
        };
        let (_, _, palette) = analyze_image(&bytes)?;
        let changed = sqlx::query("UPDATE asset_versions SET metadata=json_set(metadata,'$.palette',json(?)) WHERE account_id=? AND asset_id=? AND id=?")
            .bind(palette.to_string()).bind(account).bind(&asset.id).bind(version).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::NotFound);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
