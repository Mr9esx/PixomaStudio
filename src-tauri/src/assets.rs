use crate::{
    Error, Result, Store,
    error::{required, timestamp},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, Transaction};
use std::{
    collections::HashSet,
    fs::OpenOptions,
    io::Write,
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

pub(crate) struct AssetCreation<'a> {
    pub session_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub name: &'a str,
    pub bytes: &'a [u8],
    pub origin: &'a str,
    pub content_origin: &'a str,
    pub request_id: &'a str,
    pub source_modified_at: Option<&'a str>,
}

impl Store {
    async fn save_blob_in(
        &self,
        transaction: &mut Transaction<'_, Sqlite>,
        account: &str,
        bytes: &[u8],
    ) -> Result<String> {
        Self::check_account(account)?;
        if bytes.len() > MAX_ASSET_BYTES {
            return Err(Error::Invalid("文件超过 256 MiB"));
        }
        let hash = digest(bytes);
        let root = self.root.join("assets");
        let parent = root.join(digest(account.as_bytes()));
        if root.is_symlink() || parent.is_symlink() {
            return Err(Error::Invalid("资产目录不能使用符号链接"));
        }
        std::fs::create_dir_all(&parent)?;
        let parent = parent.canonicalize()?;
        if !parent.starts_with(self.root.canonicalize()?) {
            return Err(Error::Invalid("资产目录超出授权范围"));
        }
        let path = parent.join(&hash);
        sqlx::query("INSERT INTO blobs(account_id,digest,path,size_bytes,created_at) VALUES(?,?,?,?,?) ON CONFLICT(account_id,digest) DO NOTHING")
            .bind(account).bind(&hash).bind(path.to_str().ok_or(Error::Invalid("文件路径编码错误"))?)
            .bind(bytes.len() as i64).bind(timestamp()?).execute(&mut **transaction).await?;
        if path.exists() {
            if path.is_symlink() || digest(&std::fs::read(&path)?) != hash {
                return Err(Error::Invalid("资产文件内容校验失败"));
            }
            return Ok(hash);
        }
        let staging = parent.join(format!("{}.pending", Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&staging)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(staging, &path)?;
        Ok(hash)
    }

    pub(crate) async fn collect_unreferenced_asset_files(&self) -> Result<()> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let references = sqlx::query("SELECT account_id,digest FROM asset_versions UNION SELECT account_id,json_extract(metadata,'$.poster_digest') AS digest FROM asset_versions WHERE json_type(metadata,'$.poster_digest')='text'")
            .fetch_all(&mut *transaction)
            .await?;
        let referenced: HashSet<(String, String)> = references
            .iter()
            .map(|row| {
                (
                    digest(row.get::<String, _>("account_id").as_bytes()),
                    row.get("digest"),
                )
            })
            .collect();
        sqlx::query("DELETE FROM blobs WHERE NOT EXISTS (SELECT 1 FROM asset_versions v WHERE v.account_id=blobs.account_id AND (v.digest=blobs.digest OR json_extract(v.metadata,'$.poster_digest')=blobs.digest))")
            .execute(&mut *transaction).await?;
        let root = self.root.join("assets");
        if root.try_exists()? {
            if root.is_symlink() {
                return Err(Error::Invalid("资产目录不能使用符号链接"));
            }
            let is_digest = |value: &str| {
                value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            };
            for account in std::fs::read_dir(root)? {
                let account = account?;
                let name = account.file_name();
                let Some(account_name) = name.to_str().filter(|name| is_digest(name)) else {
                    continue;
                };
                if !account.file_type()?.is_dir() {
                    continue;
                }
                for file in std::fs::read_dir(account.path())? {
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
                    if pending
                        || (is_digest(name)
                            && !referenced.contains(&(account_name.to_owned(), name.to_owned())))
                    {
                        std::fs::remove_file(file.path())?;
                    }
                }
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn completed_asset_operation(
        &self,
        account: &str,
        request: &str,
        operation: &str,
        fingerprint: &str,
    ) -> Result<Option<Asset>> {
        let row = sqlx::query("SELECT asset_id,operation,request_digest FROM asset_operations WHERE account_id=? AND request_id=?")
            .bind(account).bind(request).fetch_optional(&self.pool).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.get::<String, _>("operation") != operation
            || row.get::<String, _>("request_digest") != fingerprint
        {
            return Err(Error::Invalid("请求标识已用于其他操作"));
        }
        Ok(Some(
            self.get_asset(account, &row.get::<String, _>("asset_id"))
                .await?,
        ))
    }

    pub async fn import_asset(
        &self,
        account: &str,
        session_id: &str,
        input: &Path,
    ) -> Result<Asset> {
        let metadata = std::fs::metadata(input)?;
        if !metadata.is_file() || metadata.len() > MAX_ASSET_BYTES as u64 {
            return Err(Error::Invalid("文件类型或大小无效"));
        }
        let name = input
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(Error::Invalid("文件名称无效"))?;
        self.create_asset_bytes(
            account,
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
        account: &str,
        session_id: &str,
        name: &str,
        content: &str,
        request_id: &str,
    ) -> Result<Asset> {
        self.create_text_asset_with_origin(account, session_id, name, content, request_id, "user")
            .await
    }

    pub(crate) async fn create_text_asset_with_origin(
        &self,
        account: &str,
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
        self.create_asset_details(
            account,
            AssetCreation {
                session_id: Some(session_id),
                project_id: None,
                name: &name,
                bytes: content.as_bytes(),
                origin,
                content_origin: "manual",
                request_id,
                source_modified_at: None,
            },
        )
        .await
    }

    pub async fn create_asset_bytes(
        &self,
        account: &str,
        session_id: Option<&str>,
        name: &str,
        bytes: &[u8],
        origin: &str,
        request_id: &str,
    ) -> Result<Asset> {
        self.create_asset_details(
            account,
            AssetCreation {
                session_id,
                project_id: None,
                name,
                bytes,
                origin,
                content_origin: origin,
                request_id,
                source_modified_at: None,
            },
        )
        .await
    }

    pub(crate) async fn create_asset_details(
        &self,
        account: &str,
        input: AssetCreation<'_>,
    ) -> Result<Asset> {
        let AssetCreation {
            session_id,
            project_id,
            name,
            bytes,
            origin,
            content_origin,
            request_id,
            source_modified_at,
        } = input;
        Self::check_account(account)?;
        required(name, "资产名称不能为空")?;
        required(request_id, "请求标识不能为空")?;
        if bytes.len() > MAX_ASSET_BYTES {
            return Err(Error::Invalid("文件超过 256 MiB"));
        }
        if session_id.is_some() && project_id.is_some() {
            return Err(Error::Invalid("会话资产不能指定其他项目"));
        }
        if let Some(project) = project_id.filter(|id| !id.is_empty()) {
            self.get_project(account, project).await?;
        }
        if let Some(date) = source_modified_at {
            time::OffsetDateTime::parse(date, &time::format_description::well_known::Rfc3339)
                .map_err(|_| Error::Invalid("文件修改日期无效"))?;
        }
        if !["user", "agent", "model", "workflow"].contains(&origin) {
            return Err(Error::Invalid("资产来源无效"));
        }
        let session = if let Some(id) = session_id {
            Some(self.get_session(account, id).await?)
        } else {
            None
        };
        let hash = digest(bytes);
        let fingerprint = digest(&serde_json::to_vec(&(
            session_id,
            project_id,
            name,
            &hash,
            origin,
            content_origin,
            source_modified_at,
        ))?);
        if let Some(asset) = self
            .completed_asset_operation(account, request_id, "create", &fingerprint)
            .await?
        {
            return Ok(asset);
        }
        let mime = infer::get(bytes)
            .map(|kind| kind.mime_type().to_owned())
            .unwrap_or_else(|| {
                mime_guess::from_path(name)
                    .first_or_octet_stream()
                    .to_string()
            });
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
        let (analysis, poster) = match kind {
            "image" => (Some(crate::asset_analysis::analyze_image(bytes)?), None),
            "video" => {
                let (width, height, palette, poster) = self.analyze_video(bytes).await?;
                (Some((width, height, palette)), Some(poster))
            }
            _ => (None, None),
        };
        let dimensions = analysis
            .as_ref()
            .map(|(width, height, _)| (*width, *height));
        let format = infer::get(bytes)
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
        self.save_blob_in(&mut transaction, account, bytes).await?;
        let mut metadata = serde_json::json!({ "source_modified_at": source_modified_at, "palette": analysis.map(|(_, _, palette)| palette) });
        if let Some(poster) = poster {
            metadata["poster_digest"] = serde_json::json!(
                self.save_blob_in(&mut transaction, account, &poster)
                    .await?
            );
        }
        sqlx::query("INSERT INTO assets(id,account_id,name,kind,origin,current_version,created_at,updated_at) VALUES(?,?,?,?,?,1,?,?)")
            .bind(&id).bind(account).bind(name).bind(kind).bind(origin).bind(&now).bind(&now).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO asset_versions(id,account_id,asset_id,version,digest,mime_type,size_bytes,format,width_px,height_px,content_origin,created_at,metadata) VALUES(?,?,?,1,?,?,?,?,?,?,?,?,?)")
            .bind(&version_id).bind(account).bind(&id).bind(&hash).bind(&mime).bind(bytes.len() as i64).bind(&format)
            .bind(dimensions.map(|(width, _)| width as i64)).bind(dimensions.map(|(_, height)| height as i64))
            .bind(content_origin).bind(&now).bind(metadata.to_string()).execute(&mut *transaction).await?;
        let project_id = session
            .as_ref()
            .and_then(|session| session.project_id.clone())
            .or_else(|| project_id.filter(|id| !id.is_empty()).map(str::to_owned));
        if let Some(session) = session {
            sqlx::query("INSERT INTO asset_usages(id,account_id,session_id,asset_id,asset_version_id,session_title_snapshot,usage_kind,created_at) VALUES(?,?,?,?,?,?,?,?)")
                .bind(Uuid::new_v4().to_string()).bind(account).bind(&session.id).bind(&id).bind(&version_id)
                .bind(&session.title).bind(if content_origin == "upload" { "uploaded" } else { "created" }).bind(&now).execute(&mut *transaction).await?;
        }
        let recorded: Option<String> = sqlx::query_scalar("INSERT INTO asset_operations(account_id,request_id,operation,request_digest,asset_id) VALUES(?,?,'create',?,?) ON CONFLICT(account_id,request_id) DO NOTHING RETURNING asset_id")
            .bind(account).bind(request_id).bind(&fingerprint).bind(&id).fetch_optional(&mut *transaction).await?;
        if recorded.is_none() {
            transaction.rollback().await?;
            return self
                .completed_asset_operation(account, request_id, "create", &fingerprint)
                .await?
                .ok_or(Error::State("重复资产请求缺少保存结果"));
        }
        sqlx::query("INSERT INTO project_assets(id,account_id,project_id,asset_id,asset_version_id,display_name,added_at,updated_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(Uuid::new_v4().to_string()).bind(account).bind(project_id).bind(&id).bind(&version_id).bind(name).bind(&now).bind(&now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_asset(account, &id).await
    }

    pub async fn get_asset(&self, account: &str, id: &str) -> Result<Asset> {
        Self::check_account(account)?;
        let row = sqlx::query("SELECT id,name,kind,origin,current_version,created_at,updated_at FROM assets WHERE account_id=? AND id=?")
            .bind(account).bind(id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let rows = sqlx::query("SELECT v.*,b.path FROM asset_versions v JOIN blobs b ON b.account_id=v.account_id AND b.digest=v.digest WHERE v.account_id=? AND v.asset_id=? ORDER BY v.version")
            .bind(account).bind(id).fetch_all(&self.pool).await?;
        let versions = rows
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
        let usages = sqlx::query("SELECT u.id,u.session_id,u.session_title_snapshot,u.usage_kind,u.run_id,u.created_at,EXISTS(SELECT 1 FROM sessions s WHERE s.account_id=u.account_id AND s.id=u.session_id) AS session_available FROM asset_usages u WHERE u.account_id=? AND u.asset_id=? ORDER BY u.created_at,u.id")
            .bind(account).bind(id).fetch_all(&self.pool).await?.iter().map(|row| serde_json::json!({
                "id": row.get::<String, _>("id"), "session_id": row.get::<String, _>("session_id"),
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

    pub async fn asset_path(
        &self,
        account: &str,
        asset_id: &str,
        version_id: &str,
    ) -> Result<PathBuf> {
        Self::check_account(account)?;
        let path: Option<String> = sqlx::query_scalar("SELECT b.path FROM asset_versions v JOIN blobs b ON b.account_id=v.account_id AND b.digest=v.digest WHERE v.account_id=? AND v.asset_id=? AND v.id=?")
            .bind(account).bind(asset_id).bind(version_id).fetch_optional(&self.pool).await?;
        let path = PathBuf::from(path.ok_or(Error::NotFound)?).canonicalize()?;
        let root = self
            .root
            .join("assets")
            .join(digest(account.as_bytes()))
            .canonicalize()?;
        if !path.starts_with(root) {
            return Err(Error::Invalid("文件路径超出授权范围"));
        }
        Ok(path)
    }

    pub async fn read_asset(
        &self,
        account: &str,
        asset_id: &str,
        version_id: &str,
    ) -> Result<Vec<u8>> {
        Ok(std::fs::read(
            self.asset_path(account, asset_id, version_id).await?,
        )?)
    }

    pub async fn update_text_asset(
        &self,
        account: &str,
        id: &str,
        content: &str,
        request_id: &str,
    ) -> Result<Asset> {
        self.update_text_asset_details(account, id, content, request_id, None)
            .await
    }

    pub(crate) async fn update_text_asset_in_session(
        &self,
        account: &str,
        session: &str,
        id: &str,
        content: &str,
        request_id: &str,
    ) -> Result<Asset> {
        self.update_text_asset_details(account, id, content, request_id, Some(session))
            .await
    }

    async fn update_text_asset_details(
        &self,
        account: &str,
        id: &str,
        content: &str,
        request_id: &str,
        session: Option<&str>,
    ) -> Result<Asset> {
        required(request_id, "请求标识不能为空")?;
        required(content, "文档内容不能为空")?;
        if content.len() > MAX_MANUAL_TEXT_BYTES {
            return Err(Error::Invalid("文档内容超过 1 MiB"));
        }
        let asset = self.get_asset(account, id).await?;
        let session = match session {
            Some(id) => Some(self.get_session(account, id).await?),
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
            .completed_asset_operation(account, request_id, "update", &fingerprint)
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
        self.save_blob_in(&mut transaction, account, content.as_bytes())
            .await?;
        let changed = sqlx::query("UPDATE assets SET current_version=current_version+1,updated_at=? WHERE account_id=? AND id=? AND current_version=?")
            .bind(&now).bind(account).bind(id).bind(asset.current_version).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            transaction.rollback().await?;
            return self
                .completed_asset_operation(account, request_id, "update", &fingerprint)
                .await?
                .ok_or(Error::Invalid("资产已更新，重新读取后编辑"));
        }
        let version_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO asset_versions(id,account_id,asset_id,version,digest,mime_type,size_bytes,format,content_origin,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(&version_id).bind(account).bind(id).bind(asset.current_version + 1).bind(hash)
            .bind(&current.mime_type).bind(content.len() as i64).bind(&current.format).bind("manual").bind(&now).execute(&mut *transaction).await?;
        if let Some(session) = &session {
            sqlx::query("INSERT INTO asset_usages(id,account_id,session_id,asset_id,asset_version_id,session_title_snapshot,usage_kind,created_at) VALUES(?,?,?,?,?,?,'created',?)")
                .bind(Uuid::new_v4().to_string()).bind(account).bind(&session.id).bind(id).bind(&version_id).bind(&session.title).bind(&now)
                .execute(&mut *transaction).await?;
            sqlx::query("INSERT INTO project_assets(id,account_id,project_id,asset_id,asset_version_id,display_name,added_at,updated_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
                .bind(Uuid::new_v4().to_string()).bind(account).bind(&session.project_id).bind(id).bind(&version_id).bind(&asset.name).bind(&now).bind(&now)
                .execute(&mut *transaction).await?;
        }
        let recorded: Option<String> = sqlx::query_scalar("INSERT INTO asset_operations(account_id,request_id,operation,request_digest,asset_id) VALUES(?,?,'update',?,?) ON CONFLICT(account_id,request_id) DO NOTHING RETURNING asset_id")
            .bind(account).bind(request_id).bind(&fingerprint).bind(id).fetch_optional(&mut *transaction).await?;
        if recorded.is_none() {
            transaction.rollback().await?;
            return self
                .completed_asset_operation(account, request_id, "update", &fingerprint)
                .await?
                .ok_or(Error::State("重复资产请求缺少保存结果"));
        }
        transaction.commit().await?;
        self.get_asset(account, id).await
    }

    pub async fn reference_asset(
        &self,
        account: &str,
        session_id: &str,
        asset_id: &str,
        version_id: &str,
        request_id: &str,
    ) -> Result<Asset> {
        self.reference_asset_input(account, session_id, serde_json::json!({ "asset_id": asset_id, "asset_version_id": version_id, "request_id": request_id })).await
    }

    pub(crate) async fn reference_asset_input(
        &self,
        account: &str,
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
        let session = self.get_session(account, session_id).await?;
        self.asset_path(account, &input.asset_id, &input.asset_version_id)
            .await?;
        let asset = self.get_asset(account, &input.asset_id).await?;
        let fingerprint = digest(&serde_json::to_vec(&(
            session_id,
            &input.asset_id,
            &input.asset_version_id,
            &input.source_project_asset_id,
        ))?);
        if let Some(previous) = self
            .completed_asset_operation(account, &input.request_id, "reference", &fingerprint)
            .await?
        {
            return Ok(previous);
        }
        let copy_source = match input.source_project_asset_id {
            Some(id) => {
                let source = self.project_asset(account, &id).await?;
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
                        self.get_project(account, project).await?.name
                    };
                    Some(serde_json::json!({ "project_id": project, "project_asset_id": id, "project_name_snapshot": name, "display_name": source["display_name"], "deleted": false }).to_string())
                }
            }
            None => None,
        };
        let now = timestamp()?;
        let mut transaction = self.pool.begin().await?;
        let recorded: Option<String> = sqlx::query_scalar("INSERT INTO asset_operations(account_id,request_id,operation,request_digest,asset_id) VALUES(?,?,'reference',?,?) ON CONFLICT(account_id,request_id) DO NOTHING RETURNING asset_id")
            .bind(account).bind(&input.request_id).bind(&fingerprint).bind(&input.asset_id).fetch_optional(&mut *transaction).await?;
        if recorded.is_none() {
            transaction.rollback().await?;
            return self
                .completed_asset_operation(account, &input.request_id, "reference", &fingerprint)
                .await?
                .ok_or(Error::State("重复资产请求缺少保存结果"));
        }
        sqlx::query("INSERT INTO project_assets(id,account_id,project_id,asset_id,asset_version_id,display_name,added_at,updated_at,copy_source) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
            .bind(Uuid::new_v4().to_string()).bind(account).bind(session.project_id).bind(&input.asset_id).bind(&input.asset_version_id)
            .bind(asset.name).bind(&now).bind(&now).bind(copy_source).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO asset_usages(id,account_id,session_id,asset_id,asset_version_id,session_title_snapshot,usage_kind,created_at) VALUES(?,?,?,?,?,?,'referenced',?) ON CONFLICT(session_id,asset_id,asset_version_id,usage_kind) DO NOTHING")
            .bind(Uuid::new_v4().to_string()).bind(account).bind(session_id).bind(&input.asset_id).bind(&input.asset_version_id)
            .bind(session.title).bind(&now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_asset(account, &input.asset_id).await
    }

    pub async fn list_session_assets(&self, account: &str, session_id: &str) -> Result<Vec<Asset>> {
        self.get_session(account, session_id).await?;
        let ids: Vec<String> = sqlx::query_scalar("SELECT DISTINCT asset_id FROM asset_usages WHERE account_id=? AND session_id=? ORDER BY asset_id")
            .bind(account).bind(session_id).fetch_all(&self.pool).await?;
        let mut assets = Vec::with_capacity(ids.len());
        for id in ids {
            assets.push(self.get_asset(account, &id).await?);
        }
        Ok(assets)
    }
}
