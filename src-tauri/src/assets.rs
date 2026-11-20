use crate::{
    Error, Result, Store,
    error::{required, timestamp},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, Transaction};
use std::{
    borrow::Cow,
    collections::HashSet,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_ASSET_BYTES: usize = 256 * 1024 * 1024;
const MAX_MANUAL_TEXT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetVersion {
    pub id: String,
    pub version: i64,
    pub mime_type: String,
    pub size_bytes: i64,
    pub metadata: Value,
    pub palette: Option<Value>,
    pub content_url: String,
    pub poster_url: Option<String>,
    pub created_at: String,
    pub format: String,
    pub width_px: Option<i64>,
    pub height_px: Option<i64>,
    pub content_origin: String,
    pub source_modified_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub origin: String,
    pub current_version: i64,
    pub versions: Vec<AssetVersion>,
    pub usages: Vec<Value>,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Copy)]
pub(crate) enum AssetContent<'a> {
    Bytes(&'a [u8]),
    File(&'a Path),
}

impl<'a> AssetContent<'a> {
    fn reader(self) -> Result<Box<dyn Read + Send + 'a>> {
        Ok(match self {
            Self::Bytes(bytes) => Box::new(io::Cursor::new(bytes)),
            Self::File(path) => Box::new(File::open(path)?),
        })
    }

    fn len(self) -> Result<u64> {
        match self {
            Self::Bytes(bytes) => Ok(bytes.len() as u64),
            Self::File(path) => {
                let metadata = std::fs::metadata(path)?;
                if !metadata.is_file() { return Err(Error::Invalid("资产来源需要文件")); }
                Ok(metadata.len())
            }
        }
    }

    fn digest(self) -> Result<String> {
        if let Self::Bytes(bytes) = self { return Ok(digest(bytes)); }
        let mut reader = self.reader()?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 65536];
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 { break; }
            hasher.update(&buffer[..count]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn header(self) -> Result<Cow<'a, [u8]>> {
        if let Self::Bytes(bytes) = self { return Ok(Cow::Borrowed(bytes)); }
        let mut header = Vec::new();
        self.reader()?.take(8192).read_to_end(&mut header)?;
        Ok(Cow::Owned(header))
    }
}

pub(crate) struct AssetCreation<'a> {
    pub session_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub name: &'a str,
    pub content: AssetContent<'a>,
    pub origin: &'a str,
    pub content_origin: &'a str,
    pub request_id: &'a str,
    pub source_modified_at: Option<&'a str>,
}

impl Store {
    async fn save_blob_in(
        &self,
        transaction: &mut Transaction<'_, Sqlite>,
        content: AssetContent<'_>,
    ) -> Result<String> {
        let size = content.len()?;
        if matches!(content, AssetContent::Bytes(_)) && size > MAX_ASSET_BYTES as u64 {
            return Err(Error::Invalid("文件超过 256 MiB"));
        }
        let size = i64::try_from(size).map_err(|_| Error::Invalid("资产文件超过大小范围"))?;
        let hash = content.digest()?;
        let parent = self.root.join("assets");
        if parent.is_symlink() {
            return Err(Error::Invalid("资产目录不能使用符号链接"));
        }
        std::fs::create_dir_all(&parent)?;
        let parent = parent.canonicalize()?;
        if !parent.starts_with(self.root.canonicalize()?) {
            return Err(Error::Invalid("资产目录超出授权范围"));
        }
        let path = parent.join(&hash);
        sqlx::query("INSERT INTO blobs(digest,path,size_bytes,created_at) VALUES(?,?,?,?) ON CONFLICT(digest) DO NOTHING")
            .bind(&hash).bind(path.to_str().ok_or(Error::Invalid("文件路径编码错误"))?)
            .bind(size).bind(timestamp()?).execute(&mut **transaction).await?;
        if path.exists() {
            if path.is_symlink() || AssetContent::File(&path).digest()? != hash {
                return Err(Error::Invalid("资产文件内容校验失败"));
            }
            return Ok(hash);
        }
        let mut staging = tempfile::Builder::new().suffix(".pending").tempfile_in(parent)?;
        let written = io::copy(&mut content.reader()?, &mut staging)?;
        staging.as_file().sync_all()?;
        if written != size as u64 || AssetContent::File(staging.path()).digest()? != hash {
            return Err(Error::Invalid("资产文件内容校验失败"));
        }
        staging.persist(path).map_err(|error| Error::Io(error.error))?;
        Ok(hash)
    }

    pub(crate) async fn collect_unreferenced_asset_files(&self) -> Result<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let references: Vec<String> = sqlx::query_scalar("SELECT digest FROM asset_versions UNION SELECT json_extract(metadata,'$.poster_digest') AS digest FROM asset_versions WHERE json_type(metadata,'$.poster_digest')='text'")
            .fetch_all(&mut *transaction)
            .await?;
        let referenced: HashSet<String> = references.into_iter().collect();
        sqlx::query("DELETE FROM blobs WHERE NOT EXISTS (SELECT 1 FROM asset_versions v WHERE v.digest=blobs.digest OR json_extract(v.metadata,'$.poster_digest')=blobs.digest)")
            .execute(&mut *transaction).await?;
        let root = self.root.join("assets");
        if root.try_exists()? {
            if root.is_symlink() {
                return Err(Error::Invalid("资产目录不能使用符号链接"));
            }
            let is_digest = |value: &str| {
                value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            };
            for file in std::fs::read_dir(root)? {
                let file = file?;
                if !file.file_type()?.is_file() {
                    continue;
                }
                let name = file.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                let pending = name
                    .strip_suffix(".pending")
                    .is_some_and(|name| Uuid::parse_str(name).is_ok());
                if pending || (is_digest(name) && !referenced.contains(name)) {
                    std::fs::remove_file(file.path())?;
                }
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn completed_asset_operation(
        &self,
        request: &str,
        operation: &str,
        fingerprint: &str,
    ) -> Result<Option<Asset>> {
        let row = sqlx::query(
            "SELECT asset_id,operation,request_digest FROM asset_operations WHERE request_id=?",
        )
        .bind(request)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.get::<String, _>("operation") != operation
            || row.get::<String, _>("request_digest") != fingerprint
        {
            return Err(Error::Invalid("请求标识已用于其他操作"));
        }
        Ok(Some(
            self.get_asset(&row.get::<String, _>("asset_id")).await?,
        ))
    }

    pub async fn import_asset(&self, session_id: &str, input: &Path) -> Result<Asset> {
        let metadata = std::fs::metadata(input)?;
        if !metadata.is_file() || metadata.len() > MAX_ASSET_BYTES as u64 {
            return Err(Error::Invalid("文件类型或大小无效"));
        }
        let name = input
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(Error::Invalid("文件名称无效"))?;
        self.create_asset_bytes(
            Some(session_id),
            name,
            &std::fs::read(input)?,
            "user",
            &Uuid::new_v4().to_string(),
        )
        .await
    }

    pub async fn create_text_asset(
        &self,
        session_id: &str,
        name: &str,
        content: &str,
        request_id: &str,
    ) -> Result<Asset> {
        self.create_text_asset_with_origin(session_id, name, content, request_id, "user")
            .await
    }

    pub(crate) async fn create_text_asset_with_origin(
        &self,
        session_id: &str,
        name: &str,
        content: &str,
        request_id: &str,
        origin: &str,
    ) -> Result<Asset> {
        required(content, "文档内容不能为空")?;
        if content.len() > MAX_MANUAL_TEXT_BYTES {
            return Err(Error::Invalid("文档内容超过 1 MiB"));
        }
        let name = if name.trim().to_lowercase().ends_with(".md") {
            name.trim().to_owned()
        } else {
            format!("{}.md", name.trim())
        };
        self.create_asset_details(AssetCreation {
            session_id: Some(session_id),
            project_id: None,
            name: &name,
            content: AssetContent::Bytes(content.as_bytes()),
            origin,
            content_origin: "manual",
            request_id,
            source_modified_at: None,
        })
        .await.map(|(asset, _)| asset)
    }

    pub async fn create_asset_bytes(
        &self,
        session_id: Option<&str>,
        name: &str,
        bytes: &[u8],
        origin: &str,
        request_id: &str,
    ) -> Result<Asset> {
        self.create_asset_details(AssetCreation {
            session_id,
            project_id: None,
            name,
            content: AssetContent::Bytes(bytes),
            origin,
            content_origin: origin,
            request_id,
            source_modified_at: None,
        })
        .await.map(|(asset, _)| asset)
    }

    pub async fn ensure_video_boundary_outputs(
        &self,
        session_id: Option<&str>,
        execution_id: &str,
        asset_id: &str,
        version_id: &str,
        output_index: i64,
    ) -> Result<Vec<Value>> {
        let local: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM local_executions WHERE id=?)")
            .bind(execution_id).fetch_one(&self.pool).await?;
        if !local {
            let online: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM online_executions WHERE id=?)")
                .bind(execution_id).fetch_one(&self.pool).await?;
            if !online { return Err(Error::NotFound); }
        }
        let asset = self.get_asset(asset_id).await?;
        if asset.kind != "video" {
            return Err(Error::Invalid("首尾帧来源需要视频资产"));
        }
        let version = asset.versions.iter().find(|version| version.id == version_id)
            .ok_or(Error::NotFound)?;
        self.ensure_video_metadata(asset_id, version_id).await?;
        let bytes = self.read_asset(asset_id, version_id).await?;
        let frames = self.extract_video_boundary_frames(&bytes).await?;
        let mut outputs = Vec::new();
        for (role, bytes) in ["first_frame", "last_frame"].into_iter().zip(frames) {
            let label = if role == "first_frame" { "首帧" } else { "尾帧" };
            let frame = self.create_asset_details(AssetCreation {
                session_id, project_id: None, name: &format!("{} · {label}.png", asset.name),
                content: AssetContent::Bytes(&bytes), origin: "workflow", content_origin: "derived",
                request_id: &format!("video-frame:{version_id}:{role}"), source_modified_at: None,
            }).await?.0;
            let frame_version = frame.versions.first().ok_or(Error::State("首尾帧资产版本缺失"))?;
            self.place_video_generation_output(execution_id,&frame.id,&frame_version.id).await?;
            let reference = serde_json::json!({"asset_id":frame.id,"version_id":frame_version.id,"url":frame_version.content_url});
            let source = serde_json::json!({"asset_id":asset_id,"version_id":version.id,"role":role});
            let output_id = format!("boundary:{version_id}:{role}");
            let hash = digest(&bytes);
            let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
            sqlx::query("UPDATE asset_versions SET metadata=json_set(metadata,?,json(?)) WHERE asset_id=? AND id=?")
                .bind(format!("$.{role}")).bind(reference.to_string()).bind(asset_id).bind(version_id)
                .execute(&mut *transaction).await?;
            sqlx::query("UPDATE asset_versions SET metadata=json_set(metadata,'$.source_video',json(?)) WHERE asset_id=? AND id=?")
                .bind(source.to_string()).bind(&frame.id).bind(&frame_version.id)
                .execute(&mut *transaction).await?;
            if local {
                sqlx::query("INSERT INTO local_execution_outputs(execution_id,output_id,field_key,output_index,sha256,size_bytes,media_type,asset_id,asset_version_id) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(execution_id,output_id) DO NOTHING")
                    .bind(execution_id).bind(&output_id).bind(role).bind(output_index).bind(&hash)
                    .bind(bytes.len() as i64).bind("image/png").bind(&frame.id).bind(&frame_version.id)
                    .execute(&mut *transaction).await?;
            }
            transaction.commit().await?;
            outputs.push(serde_json::json!({"output_id":output_id,"field_key":role,"index":output_index,
                "sha256":hash,"size_bytes":bytes.len(),"media_type":"image/png","asset_id":frame.id,"asset_version_id":frame_version.id}));
        }
        Ok(outputs)
    }

    pub(crate) async fn create_asset_details(&self, input: AssetCreation<'_>) -> Result<(Asset, bool)> {
        let AssetCreation {
            session_id,
            project_id,
            name,
            content,
            origin,
            content_origin,
            request_id,
            source_modified_at,
        } = input;
        required(name, "资产名称不能为空")?;
        required(request_id, "请求标识不能为空")?;
        let size = content.len()?;
        if matches!(content, AssetContent::Bytes(_)) && size > MAX_ASSET_BYTES as u64 {
            return Err(Error::Invalid("文件超过 256 MiB"));
        }
        if matches!(content, AssetContent::File(_)) && content_origin != "video_export" {
            return Err(Error::Invalid("文件登记仅用于视频导出"));
        }
        let size = i64::try_from(size).map_err(|_| Error::Invalid("资产文件超过大小范围"))?;
        let bytes = content.header()?;
        if session_id.is_some() && project_id.is_some() {
            return Err(Error::Invalid("会话资产不能指定其他项目"));
        }
        if let Some(project) = project_id.filter(|id| !id.is_empty()) {
            self.get_project(project).await?;
        }
        if let Some(date) = source_modified_at {
            time::OffsetDateTime::parse(date, &time::format_description::well_known::Rfc3339)
                .map_err(|_| Error::Invalid("文件修改日期无效"))?;
        }
        if !["user", "agent", "model", "workflow"].contains(&origin) {
            return Err(Error::Invalid("资产来源无效"));
        }
        let session = if let Some(id) = session_id {
            Some(self.get_session(id).await?)
        } else {
            None
        };
        let hash = content.digest()?;
        // ponytail: 导入按应用串行处理；需要并行导入时改为按项目加锁。
        let _upload_guard = if content_origin == "upload" { Some(self.asset_uploads.lock().await) } else { None };
        let fingerprint = digest(&serde_json::to_vec(&(
            session_id,
            project_id,
            name,
            &hash,
            origin,
            content_origin,
            source_modified_at,
        ))?);
        let mime = infer::get(&bytes)
            .map(|kind| kind.mime_type().to_owned())
            .unwrap_or_else(|| {
                mime_guess::from_path(name)
                    .first_or_octet_stream()
                    .to_string()
            });
        if let Some(mut asset) = self
            .completed_asset_operation(request_id, "create", &fingerprint)
            .await?
        {
            if content_origin == "upload" {
                asset.current_version = sqlx::query_scalar("SELECT version FROM asset_versions WHERE asset_id=? AND digest=? AND mime_type=? ORDER BY version LIMIT 1")
                    .bind(&asset.id).bind(&hash).bind(&mime).fetch_optional(&self.pool).await?
                    .ok_or(Error::State("导入内容版本缺失"))?;
            }
            return Ok((asset, true));
        }
        let target_project = session.as_ref().and_then(|session| session.project_id.clone())
            .or_else(|| project_id.filter(|id| !id.is_empty()).map(str::to_owned));
        if content_origin == "upload" {
            let existing: Option<(String, String, i64)> = sqlx::query_as("SELECT v.asset_id,v.id,v.version FROM project_assets p JOIN asset_versions v ON v.asset_id=p.asset_id WHERE coalesce(p.project_id,'')=? AND p.archived_at IS NULL AND v.digest=? AND v.mime_type=? ORDER BY p.added_at,p.id,v.version LIMIT 1")
                .bind(target_project.as_deref().unwrap_or("")).bind(&hash).bind(&mime).fetch_optional(&self.pool).await?;
            if let Some((asset_id, version_id, version)) = existing {
                if digest(&self.read_asset(&asset_id, &version_id).await?) != hash { return Err(Error::State("资产内容校验失败")); }
                let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
                let recorded: Option<String> = sqlx::query_scalar("INSERT INTO asset_operations(request_id,operation,request_digest,asset_id) VALUES(?,'create',?,?) ON CONFLICT(request_id) DO NOTHING RETURNING asset_id")
                    .bind(request_id).bind(&fingerprint).bind(&asset_id).fetch_optional(&mut *transaction).await?;
                if recorded.is_none() {
                    transaction.rollback().await?;
                    return self.completed_asset_operation(request_id, "create", &fingerprint).await?
                        .map(|asset| (asset, true)).ok_or(Error::State("重复资产请求缺少保存结果"));
                }
                if let Some(session) = &session {
                    sqlx::query("INSERT INTO asset_usages(id,session_id,asset_id,asset_version_id,session_title_snapshot,usage_kind,created_at) VALUES(?,?,?,?,?,'uploaded',?) ON CONFLICT(session_id,asset_id,asset_version_id,usage_kind) DO NOTHING")
                        .bind(Uuid::new_v4().to_string()).bind(&session.id).bind(&asset_id).bind(&version_id)
                        .bind(&session.title).bind(timestamp()?).execute(&mut *transaction).await?;
                }
                transaction.commit().await?;
                let mut asset = self.get_asset(&asset_id).await?;
                asset.current_version = version;
                return Ok((asset, true));
            }
        }
        let kind = if mime.starts_with("image/") {
            "image"
        } else if mime.starts_with("video/") {
            "video"
        } else if mime.starts_with("audio/") {
            "audio"
        } else if mime == "application/json" {
            "data"
        } else if mime.starts_with("text/") {
            "document"
        } else {
            "file"
        };
        if matches!(content, AssetContent::File(_)) && kind != "video" {
            return Err(Error::Invalid("导出结果需要视频文件"));
        }
        let (analysis, poster, media_info) = match kind {
            "image" => (Some(crate::asset_analysis::analyze_image(&bytes)?), None, None),
            "video" => {
                let (width, height, palette, poster, metadata) = match content {
                    AssetContent::Bytes(bytes) => self.analyze_video(bytes).await?,
                    AssetContent::File(path) => self.analyze_video_file(path).await?,
                };
                (Some((width, height, palette)), Some(poster), Some(metadata))
            }
            "audio" => (None, None, Some(self.probe_audio_metadata(&bytes).await?)),
            _ => (None, None, None),
        };
        let dimensions = analysis
            .as_ref()
            .map(|(width, height, _)| (*width, *height));
        let format = infer::get(&bytes)
            .map(|kind| kind.extension().to_owned())
            .or_else(|| {
                Path::new(name)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(str::to_lowercase)
            })
            .unwrap_or_default();
        let id = Uuid::new_v4().to_string();
        let version_id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let saved_hash = self.save_blob_in(&mut transaction, content).await?;
        if saved_hash != hash { return Err(Error::Invalid("资产内容在登记期间发生变化")); }
        let mut metadata = serde_json::json!({ "source_modified_at": source_modified_at, "palette": analysis.map(|(_, _, palette)| palette) });
        if let Some(info) = media_info {
            metadata["duration"] = info["duration"].clone();
            if kind == "video" { metadata["fps"] = info["fps"].clone(); }
        }
        if let Some(poster) = poster {
            metadata["poster_digest"] =
                serde_json::json!(self.save_blob_in(&mut transaction, AssetContent::Bytes(&poster)).await?);
        }
        sqlx::query("INSERT INTO assets(id,name,kind,origin,current_version,created_at,updated_at) VALUES(?,?,?,?,1,?,?)")
            .bind(&id).bind(name).bind(kind).bind(origin).bind(&now).bind(&now).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO asset_versions(id,asset_id,version,digest,mime_type,size_bytes,format,width_px,height_px,content_origin,created_at,metadata) VALUES(?,?,1,?,?,?,?,?,?,?,?,?)")
            .bind(&version_id).bind(&id).bind(&hash).bind(&mime).bind(size).bind(&format)
            .bind(dimensions.map(|(width, _)| width as i64)).bind(dimensions.map(|(_, height)| height as i64))
            .bind(content_origin).bind(&now).bind(metadata.to_string()).execute(&mut *transaction).await?;
        if let Some(session) = session {
            sqlx::query("INSERT INTO asset_usages(id,session_id,asset_id,asset_version_id,session_title_snapshot,usage_kind,created_at) VALUES(?,?,?,?,?,?,?)")
                .bind(Uuid::new_v4().to_string()).bind(&session.id).bind(&id).bind(&version_id)
                .bind(&session.title).bind(if content_origin == "upload" { "uploaded" } else { "created" }).bind(&now).execute(&mut *transaction).await?;
        }
        let recorded: Option<String> = sqlx::query_scalar("INSERT INTO asset_operations(request_id,operation,request_digest,asset_id) VALUES(?,'create',?,?) ON CONFLICT(request_id) DO NOTHING RETURNING asset_id")
            .bind(request_id).bind(&fingerprint).bind(&id).fetch_optional(&mut *transaction).await?;
        if recorded.is_none() {
            transaction.rollback().await?;
            return self
                .completed_asset_operation(request_id, "create", &fingerprint)
                .await?
                .map(|asset| (asset, true))
                .ok_or(Error::State("重复资产请求缺少保存结果"));
        }
        sqlx::query("INSERT INTO project_assets(id,project_id,asset_id,asset_version_id,display_name,added_at,updated_at) VALUES(?,?,?,?,?,?,?)")
            .bind(Uuid::new_v4().to_string()).bind(target_project).bind(&id).bind(&version_id).bind(name).bind(&now).bind(&now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok((self.get_asset(&id).await?, false))
    }

    pub async fn get_asset(&self, id: &str) -> Result<Asset> {
        let row = sqlx::query("SELECT id,name,kind,origin,current_version,created_at,updated_at FROM assets WHERE id=?")
            .bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let rows = sqlx::query("SELECT v.*,b.path FROM asset_versions v JOIN blobs b ON b.digest=v.digest WHERE v.asset_id=? ORDER BY v.version")
            .bind(id).fetch_all(&self.pool).await?;
        let mut versions = rows
            .iter()
            .map(|row| -> Result<AssetVersion> {
                let metadata: Value = serde_json::from_str(row.get("metadata"))?;
                let content_url = format!(
                    "{}{id}/{}",
                    if cfg!(target_os = "windows") {
                        "http://pixoma-media.localhost/"
                    } else {
                        "pixoma-media://localhost/"
                    },
                    row.get::<String, _>("id")
                );
                let poster_url = metadata["poster_digest"]
                    .as_str()
                    .map(|_| format!("{content_url}/poster"));
                Ok(AssetVersion {
                    id: row.get("id"),
                    version: row.get("version"),
                    mime_type: row.get("mime_type"),
                    size_bytes: row.get("size_bytes"),
                    source_modified_at: metadata["source_modified_at"].as_str().map(str::to_owned),
                    palette: metadata
                        .get("palette")
                        .filter(|palette| palette.is_object())
                        .cloned(),
                    metadata,
                    content_url,
                    poster_url,
                    created_at: row.get("created_at"),
                    format: row.get("format"),
                    width_px: row.get("width_px"),
                    height_px: row.get("height_px"),
                    content_origin: row.get("content_origin"),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if row.get::<String, _>("kind") == "audio" {
            for version in &mut versions {
                if version.metadata.get("duration").is_some() { continue; }
                let info = self.probe_audio_metadata(&self.read_asset(id, &version.id).await?).await?;
                sqlx::query("UPDATE asset_versions SET metadata=json_set(metadata,'$.duration',json(?)) WHERE asset_id=? AND id=?")
                    .bind(info["duration"].to_string()).bind(id).bind(&version.id).execute(&self.pool).await?;
                version.metadata["duration"] = info["duration"].clone();
            }
        }
        let usages = sqlx::query("SELECT u.id,u.session_id,u.asset_version_id,u.session_title_snapshot,u.usage_kind,u.run_id,u.created_at,EXISTS(SELECT 1 FROM sessions s WHERE s.id=u.session_id) AS session_available FROM asset_usages u WHERE u.asset_id=? ORDER BY u.created_at,u.id")
            .bind(id).fetch_all(&self.pool).await?.iter().map(|row| serde_json::json!({
                "id": row.get::<String, _>("id"), "session_id": row.get::<String, _>("session_id"),
                "asset_version_id": row.get::<String, _>("asset_version_id"),
                "session_title_snapshot": row.get::<String, _>("session_title_snapshot"), "usage_kind": row.get::<String, _>("usage_kind"),
                "run_id": row.get::<Option<String>, _>("run_id"), "created_at": row.get::<String, _>("created_at"), "session_available": row.get::<bool, _>("session_available"),
            })).collect();
        Ok(Asset {
            id: row.get("id"),
            name: row.get("name"),
            kind: row.get("kind"),
            origin: row.get("origin"),
            current_version: row.get("current_version"),
            versions,
            usages,
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    pub async fn asset_path(&self, asset_id: &str, version_id: &str) -> Result<PathBuf> {
        let path: Option<String> = sqlx::query_scalar("SELECT b.path FROM asset_versions v JOIN blobs b ON b.digest=v.digest WHERE v.asset_id=? AND v.id=?")
            .bind(asset_id).bind(version_id).fetch_optional(&self.pool).await?;
        let path = PathBuf::from(path.ok_or(Error::NotFound)?).canonicalize()?;
        let root = self.root.join("assets").canonicalize()?;
        if !path.starts_with(root) {
            return Err(Error::Invalid("文件路径超出授权范围"));
        }
        Ok(path)
    }

    pub async fn read_asset(&self, asset_id: &str, version_id: &str) -> Result<Vec<u8>> {
        Ok(std::fs::read(self.asset_path(asset_id, version_id).await?)?)
    }

    pub async fn update_text_asset(
        &self,
        id: &str,
        content: &str,
        request_id: &str,
    ) -> Result<Asset> {
        self.update_text_asset_details(id, content, request_id, None, None)
            .await
    }

    pub(crate) async fn update_text_asset_in_session(
        &self,
        session: &str,
        id: &str,
        content: &str,
        request_id: &str,
    ) -> Result<Asset> {
        self.update_text_asset_details(id, content, request_id, Some(session), None)
            .await
    }

    pub(crate) async fn update_text_asset_expected(&self, session: &str, id: &str, content: &str, request_id: &str, expected: i64) -> Result<Asset> {
        self.update_text_asset_details(id, content, request_id, Some(session), Some(expected)).await
    }

    async fn update_text_asset_details(
        &self,
        id: &str,
        content: &str,
        request_id: &str,
        session: Option<&str>,
        expected_version: Option<i64>,
    ) -> Result<Asset> {
        required(request_id, "请求标识不能为空")?;
        required(content, "文档内容不能为空")?;
        if content.len() > MAX_MANUAL_TEXT_BYTES {
            return Err(Error::Invalid("文档内容超过 1 MiB"));
        }
        let asset = self.get_asset(id).await?;
        if expected_version.is_some_and(|expected| expected != asset.current_version) {
            return Err(Error::Conflict("视频项目已更新，重新读取后编辑".to_owned()));
        }
        let session = match session {
            Some(id) => Some(self.get_session(id).await?),
            None => None,
        };
        if asset.kind != "document"
            || !asset
                .versions
                .first()
                .is_some_and(|version| version.content_origin == "manual")
        {
            return Err(Error::Invalid("资产不支持文本编辑"));
        }
        let hash = digest(content.as_bytes());
        let fingerprint = match &session {
            Some(session) => digest(&serde_json::to_vec(&(id, &hash, &session.id))?),
            None => digest(&serde_json::to_vec(&(id, &hash))?),
        };
        if let Some(previous) = self
            .completed_asset_operation(request_id, "update", &fingerprint)
            .await?
        {
            return Ok(previous);
        }
        let current = asset
            .versions
            .last()
            .ok_or(Error::Invalid("资产版本不存在"))?;
        let now = timestamp()?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        self.save_blob_in(&mut transaction, AssetContent::Bytes(content.as_bytes()))
            .await?;
        let changed = sqlx::query("UPDATE assets SET current_version=current_version+1,updated_at=? WHERE id=? AND current_version=?")
            .bind(&now).bind(id).bind(asset.current_version).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            transaction.rollback().await?;
            return self
                .completed_asset_operation(request_id, "update", &fingerprint)
                .await?
                .ok_or(Error::Invalid("资产已更新，重新读取后编辑"));
        }
        let version_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO asset_versions(id,asset_id,version,digest,mime_type,size_bytes,format,content_origin,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(&version_id).bind(id).bind(asset.current_version + 1).bind(hash)
            .bind(&current.mime_type).bind(content.len() as i64).bind(&current.format).bind("manual").bind(&now).execute(&mut *transaction).await?;
        if let Some(session) = &session {
            sqlx::query("INSERT INTO asset_usages(id,session_id,asset_id,asset_version_id,session_title_snapshot,usage_kind,created_at) VALUES(?,?,?,?,?,'created',?)")
                .bind(Uuid::new_v4().to_string()).bind(&session.id).bind(id).bind(&version_id).bind(&session.title).bind(&now)
                .execute(&mut *transaction).await?;
            sqlx::query("INSERT INTO project_assets(id,project_id,asset_id,asset_version_id,display_name,added_at,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
                .bind(Uuid::new_v4().to_string()).bind(&session.project_id).bind(id).bind(&version_id).bind(&asset.name).bind(&now).bind(&now)
                .execute(&mut *transaction).await?;
        }
        let recorded: Option<String> = sqlx::query_scalar("INSERT INTO asset_operations(request_id,operation,request_digest,asset_id) VALUES(?,'update',?,?) ON CONFLICT(request_id) DO NOTHING RETURNING asset_id")
            .bind(request_id).bind(&fingerprint).bind(id).fetch_optional(&mut *transaction).await?;
        if recorded.is_none() {
            transaction.rollback().await?;
            return self
                .completed_asset_operation(request_id, "update", &fingerprint)
                .await?
                .ok_or(Error::State("重复资产请求缺少保存结果"));
        }
        transaction.commit().await?;
        self.get_asset(id).await
    }

    pub async fn reference_asset(
        &self,
        session_id: &str,
        asset_id: &str,
        version_id: &str,
        request_id: &str,
    ) -> Result<Asset> {
        self.reference_asset_input(session_id, serde_json::json!({ "asset_id": asset_id, "asset_version_id": version_id, "request_id": request_id })).await
    }

    pub(crate) async fn reference_asset_input(
        &self,
        session_id: &str,
        document: Value,
    ) -> Result<Asset> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ReferenceInput {
            asset_id: String,
            asset_version_id: String,
            request_id: String,
            source_project_asset_id: Option<String>,
        }
        let input: ReferenceInput =
            serde_json::from_value(document).map_err(|_| Error::Invalid("资产引用字段无效"))?;
        required(&input.request_id, "请求标识不能为空")?;
        let session = self.get_session(session_id).await?;
        self.asset_path(&input.asset_id, &input.asset_version_id)
            .await?;
        let asset = self.get_asset(&input.asset_id).await?;
        let fingerprint = digest(&serde_json::to_vec(&(
            session_id,
            &input.asset_id,
            &input.asset_version_id,
            &input.source_project_asset_id,
        ))?);
        if let Some(previous) = self
            .completed_asset_operation(&input.request_id, "reference", &fingerprint)
            .await?
        {
            return Ok(previous);
        }
        let copy_source = match input.source_project_asset_id {
            Some(id) => {
                let source = self.project_asset(&id).await?;
                if source["asset_id"] != input.asset_id {
                    return Err(Error::NotFound);
                }
                let project = crate::local_api::text(&source, "project_id")?;
                if project == session.project_id.as_deref().unwrap_or("") {
                    None
                } else {
                    let name = if project.is_empty() {
                        String::new()
                    } else {
                        self.get_project(project).await?.name
                    };
                    Some(serde_json::json!({ "project_id": project, "project_asset_id": id, "project_name_snapshot": name, "display_name": source["display_name"], "deleted": false }).to_string())
                }
            }
            None => None,
        };
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        let recorded: Option<String> = sqlx::query_scalar("INSERT INTO asset_operations(request_id,operation,request_digest,asset_id) VALUES(?,'reference',?,?) ON CONFLICT(request_id) DO NOTHING RETURNING asset_id")
            .bind(&input.request_id).bind(&fingerprint).bind(&input.asset_id).fetch_optional(&mut *transaction).await?;
        if recorded.is_none() {
            transaction.rollback().await?;
            return self
                .completed_asset_operation(&input.request_id, "reference", &fingerprint)
                .await?
                .ok_or(Error::State("重复资产请求缺少保存结果"));
        }
        sqlx::query("INSERT INTO project_assets(id,project_id,asset_id,asset_version_id,display_name,added_at,updated_at,copy_source) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
            .bind(Uuid::new_v4().to_string()).bind(session.project_id).bind(&input.asset_id).bind(&input.asset_version_id)
            .bind(asset.name).bind(&now).bind(&now).bind(copy_source).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO asset_usages(id,session_id,asset_id,asset_version_id,session_title_snapshot,usage_kind,created_at) VALUES(?,?,?,?,?,'referenced',?) ON CONFLICT(session_id,asset_id,asset_version_id,usage_kind) DO NOTHING")
            .bind(Uuid::new_v4().to_string()).bind(session_id).bind(&input.asset_id).bind(&input.asset_version_id)
            .bind(session.title).bind(&now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_asset(&input.asset_id).await
    }

    pub async fn list_session_assets(&self, session_id: &str) -> Result<Vec<Asset>> {
        self.get_session(session_id).await?;
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT asset_id FROM asset_usages WHERE session_id=? ORDER BY asset_id",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        let mut assets = Vec::with_capacity(ids.len());
        for id in ids {
            assets.push(self.get_asset(&id).await?);
        }
        Ok(assets)
    }
}
