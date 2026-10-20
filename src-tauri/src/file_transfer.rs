use crate::{Error, Result, Store, assets::AssetCreation, local_api::text};
use serde::Deserialize;
use serde_json::Value;
use std::io::{Cursor, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UploadMetadata {
    file_name: String,
    session_id: Option<String>,
    project_id: Option<String>,
    request_id: String,
    source_modified_at: Option<String>,
    purpose: Option<UploadPurpose>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum UploadPurpose {
    ExpertAvatar,
}

pub struct AssetExport {
    pub name: String,
    pub bytes: Vec<u8>,
}

fn filename(name: &str) -> Result<String> {
    let name = name
        .rsplit(['/', '\\'])
        .next()
        .ok_or(Error::Invalid("文件名称无效"))?;
    let name: String = name
        .chars()
        .map(|character| {
            if character.is_control() || ['<', '>', ':', '"', '|', '?', '*'].contains(&character) {
                '_'
            } else {
                character
            }
        })
        .collect();
    let name = name.trim_matches([' ', '.']);
    if name.is_empty() || name.len() > 255 {
        return Err(Error::Invalid("文件名称无效或超过 255 字节"));
    }
    Ok(name.to_owned())
}

impl Store {
    pub async fn upload_asset(
        &self,
        account: &str,
        metadata: Value,
        bytes: &[u8],
    ) -> Result<Value> {
        let metadata: UploadMetadata = serde_json::from_value(metadata)?;
        let name = filename(&metadata.file_name)?;
        if matches!(metadata.purpose, Some(UploadPurpose::ExpertAvatar)) {
            if bytes.len() > 5 << 20 {
                return Err(Error::Invalid("选择 5 MiB 以内的头像图片"));
            }
            let image =
                infer::get(bytes).is_some_and(|kind| kind.mime_type().starts_with("image/"));
            if !image {
                return Err(Error::Invalid("头像需要使用图片"));
            }
            image::ImageReader::new(Cursor::new(bytes))
                .with_guessed_format()?
                .decode()
                .map_err(|_| Error::Invalid("头像图片格式无效"))?;
        }
        if metadata.session_id.is_none() && metadata.project_id.is_none() {
            return Err(Error::Invalid("请选择会话或项目"));
        }
        let asset = self
            .create_asset_details(
                account,
                AssetCreation {
                    session_id: metadata.session_id.as_deref(),
                    project_id: metadata.project_id.as_deref(),
                    name: &name,
                    bytes,
                    origin: "user",
                    content_origin: "upload",
                    request_id: &metadata.request_id,
                    source_modified_at: metadata.source_modified_at.as_deref(),
                },
            )
            .await?;
        let placement: String = sqlx::query_scalar("SELECT id FROM project_assets WHERE account_id=? AND asset_id=? AND coalesce(project_id,'')=?")
            .bind(account).bind(&asset.id).bind(match metadata.session_id {
                Some(session) => self.get_session(account, &session).await?.project_id.unwrap_or_default(),
                None => metadata.project_id.ok_or(Error::Invalid("缺少项目标识"))?,
            }).fetch_one(&self.pool).await?;
        let mut document = serde_json::to_value(asset)?;
        document["project_asset_id"] = Value::String(placement);
        Ok(document)
    }

    pub async fn export_assets(&self, account: &str, ids: &[String]) -> Result<AssetExport> {
        Self::check_account(account)?;
        if ids.is_empty() || ids.len() > 100 {
            return Err(Error::Invalid("请选择 1 至 100 项资产"));
        }
        let mut files = Vec::with_capacity(ids.len());
        let mut total = 0_u64;
        for id in ids {
            let placement = self.project_asset(account, id).await?;
            let size = placement["version"]["size_bytes"]
                .as_u64()
                .ok_or(Error::State("资产文件大小无效"))?;
            total = total
                .checked_add(size)
                .ok_or(Error::Invalid("导出文件超过大小限制"))?;
            if total > 512 * 1024 * 1024 {
                return Err(Error::Invalid("一次最多导出 512 MiB 文件"));
            }
            files.push((
                filename(text(&placement, "display_name")?)?,
                text(&placement, "asset_id")?.to_owned(),
                text(&placement, "asset_version_id")?.to_owned(),
            ));
        }
        if let [(name, asset, version)] = files.as_slice() {
            return Ok(AssetExport {
                name: name.clone(),
                bytes: self.read_asset(account, asset, version).await?,
            });
        }
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (index, (name, asset, version)) in files.iter().enumerate() {
            archive.start_file(
                format!("{:03}-{name}", index + 1),
                zip::write::SimpleFileOptions::default().unix_permissions(0o600),
            )?;
            archive.write_all(&self.read_asset(account, asset, version).await?)?;
        }
        Ok(AssetExport {
            name: "assets.zip".to_owned(),
            bytes: archive.finish()?.into_inner(),
        })
    }
}
