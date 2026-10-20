use crate::{
    Error, Result, Store,
    assets::digest,
    error::timestamp,
    local_api::{optional_text, text},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use gray_matter::{Matter, engine::YAML};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    io::{Cursor, Read},
};
use uuid::Uuid;

const MAX_FILES: usize = 128;
const MAX_FILE_BYTES: usize = 256 << 10;
const MAX_PACKAGE_BYTES: usize = 1 << 20;

pub(crate) fn requested_skills(input: &Value) -> Result<Vec<(String, Option<String>)>> {
    let mut selected = BTreeMap::new();
    if let Some(ids) = input.get("skill_ids").filter(|ids| !ids.is_null()) {
        for id in ids.as_array().ok_or(Error::Invalid("技能标识需要数组"))? {
            let id = id
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or(Error::Invalid("技能标识无效"))?;
            selected.insert(id.to_owned(), None);
        }
    }
    if let Some(parts) = input.get("parts").and_then(Value::as_array) {
        for part in parts.iter().filter(|part| part["type"] == "skill_ref") {
            let id = text(part, "skill_id")?;
            if id.is_empty() {
                return Err(Error::Invalid("技能标识不能为空"));
            }
            let version = part
                .get("version")
                .filter(|version| !version.is_null())
                .map(|version| version.as_str().ok_or(Error::Invalid("技能版本号无效")))
                .transpose()?;
            if let Some(version) = version {
                stable_version(version)?;
                if selected
                    .get(id)
                    .and_then(Option::as_deref)
                    .is_some_and(|previous| previous != version)
                {
                    return Err(Error::Invalid("同一技能需要引用相同版本"));
                }
                selected.insert(id.to_owned(), Some(version.to_owned()));
            } else {
                selected.entry(id.to_owned()).or_insert(None);
            }
        }
    }
    Ok(selected.into_iter().collect())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkillFile {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub binary: bool,
    #[serde(default)]
    pub directory: bool,
}

#[derive(Deserialize)]
struct SkillMetadata {
    name: String,
    description: String,
}

fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 1024
        || path.contains(['\\', ':', '\0'])
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(Error::Invalid("技能文件路径无效"));
    }
    Ok(())
}

fn validate_files(files: &[SkillFile]) -> Result<(String, String, String)> {
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(Error::Invalid("技能包需要包含 1 至 128 项文件或目录"));
    }
    let mut paths = HashSet::new();
    let mut bytes = 0;
    for file in files {
        validate_path(&file.path)?;
        if !paths.insert(&file.path) {
            return Err(Error::Invalid("技能文件路径重复"));
        }
        if file.directory && (!file.content.is_empty() || file.binary) {
            return Err(Error::Invalid("技能目录不能包含文件内容"));
        }
        let size = if file.binary {
            STANDARD
                .decode(&file.content)
                .map_err(|_| Error::Invalid("技能文件 Base64 编码无效"))?
                .len()
        } else {
            file.content.len()
        };
        if size > MAX_FILE_BYTES {
            return Err(Error::Invalid("技能文件超过 256 KiB"));
        }
        bytes += size;
    }
    if bytes > MAX_PACKAGE_BYTES {
        return Err(Error::Invalid("技能包超过 1 MiB"));
    }
    for file in files.iter().filter(|file| !file.directory) {
        let prefix = format!("{}/", file.path);
        if files.iter().any(|other| other.path.starts_with(&prefix)) {
            return Err(Error::Invalid("技能文件与目录路径冲突"));
        }
    }
    let manifest = files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .filter(|file| !file.directory && !file.binary)
        .ok_or(Error::Invalid("技能包必须包含文本文件 SKILL.md"))?;
    let parsed = Matter::<YAML>::new()
        .parse::<SkillMetadata>(&manifest.content)
        .map_err(|_| Error::Invalid("SKILL.md YAML 元数据无效"))?;
    let metadata = parsed
        .data
        .ok_or(Error::Invalid("SKILL.md 缺少 YAML 元数据"))?;
    let name = metadata.name;
    if name.is_empty()
        || name.len() > 64
        || name.starts_with('-')
        || name.ends_with('-')
        || name.contains("--")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(Error::Invalid(
            "技能名称需要 1 至 64 位小写字母、数字和连字符",
        ));
    }
    let description = metadata.description.trim().to_owned();
    if description.is_empty()
        || description.chars().count() > 1024
        || parsed.content.trim().is_empty()
    {
        return Err(Error::Invalid("技能说明或操作说明无效"));
    }
    Ok((name, description, manifest.content.clone()))
}

pub(crate) fn stable_version(value: &str) -> Result<semver::Version> {
    let version = semver::Version::parse(value).map_err(|_| Error::Invalid("版本号需要 x.y.z"))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(Error::Invalid("版本号需要 x.y.z"));
    }
    Ok(version)
}

impl Store {
    pub(crate) async fn load_run_skill(
        &self,
        account: &str,
        tool_call_id: &str,
        input: &Value,
    ) -> Result<Value> {
        let document: String = sqlx::query_scalar("SELECT r.context_snapshot FROM runs r JOIN tool_calls t ON t.run_id=r.id AND t.account_id=r.account_id WHERE t.account_id=? AND t.id=?")
            .bind(account).bind(tool_call_id).fetch_optional(&self.pool).await?.ok_or(Error::NotFound)?;
        let context: crate::context::RunContext = serde_json::from_str(&document)?;
        let id = text(input, "skill_id")?;
        let skill = context
            .skills
            .iter()
            .find(|skill| skill["id"] == id)
            .ok_or(Error::NotFound)?;
        let path = match input.get("path") {
            Some(path) => path.as_str().ok_or(Error::Invalid("技能文件路径无效"))?,
            None => "SKILL.md",
        };
        let offset = match input.get("offset") {
            Some(offset) => {
                usize::try_from(offset.as_u64().ok_or(Error::Invalid("技能读取位置无效"))?)
                    .map_err(|_| Error::Invalid("技能读取位置超过范围"))?
            }
            None => 0,
        };
        let mut files: Vec<SkillFile> = if skill["files"].is_null() {
            Vec::new()
        } else {
            serde_json::from_value(skill["files"].clone())?
        };
        if files.is_empty() {
            files.push(SkillFile {
                path: "SKILL.md".to_owned(),
                content: text(skill, "prompt")?.to_owned(),
                binary: false,
                directory: false,
            });
        }
        let mut readable: Vec<&str> = files
            .iter()
            .filter(|file| !file.binary && !file.directory)
            .map(|file| file.path.as_str())
            .collect();
        readable.sort_by_key(|path| (*path != "SKILL.md", *path));
        if path == "." {
            if offset > readable.len() {
                return Err(Error::Invalid("技能目录读取位置超过范围"));
            }
            let end = offset.saturating_add(20).min(readable.len());
            return Ok(
                json!({ "skill_id": id, "name": skill["name"], "version": skill["version"],
                    "path": path, "offset": offset, "content": "", "file_count": readable.len(),
                    "readable_files": readable[offset..end], "truncated": end < readable.len(),
                    "next": (end < readable.len()).then(|| json!({ "skill_id": id, "path": ".", "offset": end })),
                }),
            );
        }
        validate_path(path)?;
        let file = files
            .iter()
            .find(|file| file.path == path)
            .ok_or(Error::NotFound)?;
        if file.binary || file.directory {
            return Err(Error::Invalid("所选技能路径需要文本文件"));
        }
        let total = file.content.chars().count();
        if offset > total {
            return Err(Error::Invalid("技能文件读取位置超过范围"));
        }
        let content: String = file.content.chars().skip(offset).take(8000).collect();
        let end = offset + content.chars().count();
        Ok(
            json!({ "skill_id": id, "name": skill["name"], "version": skill["version"],
                "path": path, "offset": offset, "content": content, "file_count": readable.len(),
                "truncated": end < total, "next": (end < total).then(|| json!({ "skill_id": id, "path": path, "offset": end })),
            }),
        )
    }

    pub(crate) async fn cache_official_skill(
        &self,
        account: &str,
        mut current: Value,
        mut version: Value,
        source_url: &str,
    ) -> Result<()> {
        Self::check_account(account)?;
        let id = text(&current, "id")?.to_owned();
        let version_number = text(&version, "version")?.to_owned();
        stable_version(&version_number)?;
        for item in [&current, &version] {
            let prompt = text(item, "prompt")?;
            if prompt.trim().is_empty() || prompt.len() > MAX_FILE_BYTES {
                return Err(Error::Invalid("官方技能操作说明无效或超过 256 KiB"));
            }
            if let Some(files) = item["files"].as_array().filter(|files| !files.is_empty()) {
                let files: Vec<SkillFile> = serde_json::from_value(json!(files))
                    .map_err(|_| Error::Invalid("官方技能文件内容无效"))?;
                let (name, description, manifest) = validate_files(&files)?;
                if name != text(item, "name")?
                    || description != text(item, "description")?
                    || manifest != prompt
                {
                    return Err(Error::Invalid("官方技能文件与元数据不一致"));
                }
            }
        }
        let content_digest = digest(&serde_json::to_vec(&json!({
            "name": version["name"], "description": version["description"], "prompt": version["prompt"], "files": version["files"],
        }))?);
        for item in [&mut current, &mut version] {
            item["id"] = json!(id);
            item["account_id"] = json!(account);
            item["publisher_source"] = item["source"].clone();
            item["source"] = json!("official");
            item["source_url"] = json!(source_url);
            item["received_at"] = json!(timestamp()?);
        }
        version["content_digest"] = json!(content_digest);
        let mut transaction = self.pool.begin().await?;
        let changed = sqlx::query("INSERT INTO skills(account_id,id,source,document) VALUES(?,?,'official',?) ON CONFLICT(account_id,id) DO UPDATE SET document=excluded.document WHERE skills.source='official'")
            .bind(account).bind(&id).bind(current.to_string()).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Invalid("官方技能标识与个人技能重复"));
        }
        let existing: Option<String> = sqlx::query_scalar("SELECT content_digest FROM skill_versions WHERE account_id=? AND skill_id=? AND version=?")
            .bind(account).bind(&id).bind(&version_number).fetch_optional(&mut *transaction).await?;
        if existing
            .as_ref()
            .is_some_and(|existing| existing != &content_digest)
        {
            return Err(Error::State("官方技能同一版本的内容发生变化"));
        }
        sqlx::query("INSERT INTO skill_versions(account_id,skill_id,version,document,content_digest) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING")
            .bind(account).bind(&id).bind(version_number).bind(version.to_string()).bind(content_digest).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn list_skills(&self, account: &str) -> Result<Value> {
        Self::check_account(account)?;
        let rows: Vec<String> = sqlx::query_scalar("SELECT document FROM skills WHERE account_id=? AND source='user' ORDER BY json_extract(document,'$.updated_at') DESC,id")
            .bind(account).fetch_all(&self.pool).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let mut item: Value = serde_json::from_str(&row)?;
            self.apply_publication_state(account, "skill", &mut item)
                .await?;
            let fields = item
                .as_object_mut()
                .ok_or(Error::State("技能记录格式无效"))?;
            fields.remove("files");
            fields.remove("prompt");
            items.push(item);
        }
        Ok(json!(items))
    }

    pub(crate) async fn get_skill(&self, account: &str, id: &str) -> Result<Value> {
        Self::check_account(account)?;
        let document: String =
            sqlx::query_scalar("SELECT document FROM skills WHERE account_id=? AND id=?")
                .bind(account)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Error::NotFound)?;
        let mut asset = serde_json::from_str(&document)?;
        self.apply_publication_state(account, "skill", &mut asset)
            .await?;
        Ok(asset)
    }

    pub(crate) async fn skill_versions(&self, account: &str, id: &str) -> Result<Value> {
        self.get_skill(account, id).await?;
        let documents: Vec<String> = sqlx::query_scalar("SELECT document FROM skill_versions WHERE account_id=? AND skill_id=? ORDER BY json_extract(document,'$.created_at') DESC,version DESC")
            .bind(account).bind(id).fetch_all(&self.pool).await?;
        Ok(json!(
            documents
                .into_iter()
                .map(|document| serde_json::from_str::<Value>(&document))
                .collect::<std::result::Result<Vec<_>, _>>()?
        ))
    }

    pub(crate) async fn skill_version(
        &self,
        account: &str,
        id: &str,
        version: &str,
    ) -> Result<Value> {
        self.get_skill(account, id).await?;
        stable_version(version)?;
        let document: String = sqlx::query_scalar(
            "SELECT document FROM skill_versions WHERE account_id=? AND skill_id=? AND version=?",
        )
        .bind(account)
        .bind(id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)?;
        Ok(serde_json::from_str(&document)?)
    }

    pub(crate) async fn save_skill(
        &self,
        account: &str,
        id: Option<&str>,
        input: Value,
    ) -> Result<Value> {
        Self::check_account(account)?;
        let previous = match id {
            Some(id) => Some(self.get_skill(account, id).await?),
            None => None,
        };
        if previous
            .as_ref()
            .is_some_and(|item| item["source"] != "user")
        {
            return Err(Error::Invalid("官方技能版本只允许读取"));
        }
        let name = text(&input, "name")?.trim();
        let description = text(&input, "description")?.trim();
        if name.is_empty() || description.is_empty() {
            return Err(Error::Invalid("技能名称和说明不能为空"));
        }
        let files = input
            .get("files")
            .filter(|value| !value.is_null())
            .map(|files| serde_json::from_value::<Vec<SkillFile>>(files.clone()))
            .transpose()
            .map_err(|_| Error::Invalid("技能文件内容无效"))?;
        let prompt = match &files {
            Some(files) => {
                let (manifest_name, manifest_description, prompt) = validate_files(files)?;
                if manifest_name != name || manifest_description != description {
                    return Err(Error::Invalid("技能名称或说明与 SKILL.md 不一致"));
                }
                prompt
            }
            None => {
                let prompt = text(&input, "prompt")?;
                if prompt.trim().is_empty() || prompt.len() > MAX_FILE_BYTES {
                    return Err(Error::Invalid("技能操作说明无效或超过 256 KiB"));
                }
                prompt.to_owned()
            }
        };
        let version = optional_text(&input, "version", "1.0.0")?;
        let notes = optional_text(&input, "notes", "")?;
        let parsed_version = stable_version(version)?;
        if let Some(previous) = &previous {
            if input["updated_at"] != previous["updated_at"] {
                return Err(Error::Invalid("技能已更新，请重新读取"));
            }
            if parsed_version <= stable_version(text(previous, "version")?)? {
                return Err(Error::Invalid("新版本号需要大于当前版本号"));
            }
        }
        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = timestamp()?;
        let enabled = input
            .get("enabled")
            .or_else(|| previous.as_ref().and_then(|item| item.get("enabled")))
            .and_then(Value::as_bool)
            .ok_or(Error::Invalid("技能启停状态无效"))?;
        let document = json!({
            "id": id, "account_id": account, "name": name, "description": description, "prompt": prompt,
            "files": files, "version": version, "enabled": enabled, "source": "user", "published": false,
            "published_version": "", "market_locked": false, "market_lock_reason": "", "available": enabled,
            "created_at": previous.as_ref().map(|item| item["created_at"].clone()).unwrap_or_else(|| json!(now)), "updated_at": now,
        });
        let mut saved_version = document.clone();
        saved_version["created_at"] = json!(now);
        saved_version["notes"] = json!(notes);
        saved_version["created_by"] = json!(account);
        saved_version["created_by_type"] = json!("studio_user");
        let mut transaction = self.pool.begin().await?;
        if let Some(previous) = &previous {
            let changed = sqlx::query("UPDATE skills SET document=? WHERE account_id=? AND id=? AND json_extract(document,'$.updated_at')=?")
                .bind(document.to_string()).bind(account).bind(&id).bind(text(previous, "updated_at")?).execute(&mut *transaction).await?;
            if changed.rows_affected() != 1 {
                return Err(Error::Invalid("技能已更新，请重新读取"));
            }
        } else {
            sqlx::query("INSERT INTO skills(account_id,id,source,document) VALUES(?,?,'user',?)")
                .bind(account)
                .bind(&id)
                .bind(document.to_string())
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query("INSERT INTO skill_versions(account_id,skill_id,version,document,content_digest) VALUES(?,?,?,?,?)")
            .bind(account).bind(&id).bind(version).bind(saved_version.to_string()).bind(digest(&serde_json::to_vec(&json!({ "name": name, "description": description, "prompt": prompt, "files": files }))?))
            .execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_skill(account, &id).await
    }

    pub(crate) async fn set_skill_enabled(
        &self,
        account: &str,
        id: &str,
        enabled: bool,
    ) -> Result<Value> {
        let mut document = self.get_skill(account, id).await?;
        if document["source"] != "user" {
            return Err(Error::Invalid("官方技能版本只允许读取"));
        }
        let previous_updated_at = text(&document, "updated_at")?.to_owned();
        document["enabled"] = json!(enabled);
        document["available"] = json!(enabled);
        document["updated_at"] = json!(timestamp()?);
        let changed = sqlx::query("UPDATE skills SET document=? WHERE account_id=? AND id=? AND json_extract(document,'$.updated_at')=?")
            .bind(document.to_string()).bind(account).bind(id).bind(previous_updated_at).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Invalid("技能已更新，请重新读取"));
        }
        Ok(document)
    }

    pub fn inspect_skill_zip(bytes: &[u8]) -> Result<Value> {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err(Error::Invalid("技能压缩文件超过 1 MiB"));
        }
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        if archive.len() > MAX_FILES {
            return Err(Error::Invalid("技能压缩文件超过 128 项"));
        }
        let mut files = Vec::with_capacity(archive.len());
        let mut total = 0;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let directory = entry.is_dir();
            let path = entry.name()?.trim_end_matches('/').to_owned();
            validate_path(&path)?;
            if entry.enclosed_name().is_none() || entry.is_symlink() {
                return Err(Error::Invalid("技能压缩文件包含无效路径或符号链接"));
            }
            if entry.size() > MAX_FILE_BYTES as u64 {
                return Err(Error::Invalid("技能文件超过 256 KiB"));
            }
            let mut content = Vec::new();
            (&mut entry)
                .take((MAX_FILE_BYTES + 1) as u64)
                .read_to_end(&mut content)?;
            total += content.len();
            if content.len() > MAX_FILE_BYTES || total > MAX_PACKAGE_BYTES {
                return Err(Error::Invalid("技能包超过文件或总大小限制"));
            }
            let (content, binary) = match String::from_utf8(content) {
                Ok(content) => (content, false),
                Err(error) => (STANDARD.encode(error.into_bytes()), true),
            };
            files.push(SkillFile {
                path,
                content,
                binary,
                directory,
            });
        }
        if !files.iter().any(|file| file.path == "SKILL.md") {
            let roots: HashSet<&str> = files
                .iter()
                .filter(|file| !file.directory)
                .map(|file| file.path.split('/').next().expect("已验证技能路径"))
                .collect();
            if roots.len() == 1 {
                let prefix = format!(
                    "{}/",
                    roots
                        .iter()
                        .next()
                        .ok_or(Error::Invalid("技能压缩文件没有文件"))?
                );
                files.retain(|file| format!("{}/", file.path) != prefix);
                for file in &mut files {
                    file.path = file
                        .path
                        .strip_prefix(&prefix)
                        .ok_or(Error::Invalid("技能目录结构无效"))?
                        .to_owned();
                }
            }
        }
        let (name, description, prompt) = validate_files(&files)?;
        Ok(json!({ "name": name, "description": description, "prompt": prompt, "files": files }))
    }
}
