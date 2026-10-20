use super::PlatformClient;
use crate::{
    Error, LocalRequest, Result, Store,
    experts::{ExpertContent, ExpertPart},
    local_api::text,
};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PublicationInput {
    request_id: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    reason: String,
}

fn version_content(kind: &str, value: &Value) -> Result<Value> {
    if kind == "expert" {
        let mut content: ExpertContent = serde_json::from_value(value.clone())?;
        content.validate()?;
        for part in content.prompt_parts.iter_mut().chain(
            content
                .sop
                .iter_mut()
                .flat_map(|step| &mut step.instruction_parts),
        ) {
            match part {
                ExpertPart::Skill { name, .. } | ExpertPart::Workflow { name, .. }
                    if name.as_ref().is_some_and(String::is_empty) =>
                {
                    *name = None
                }
                _ => {}
            }
        }
        return Ok(serde_json::to_value(content)?);
    }
    let mut files: Vec<crate::skills::SkillFile> =
        match value.get("files").filter(|files| !files.is_null()) {
            Some(files) => serde_json::from_value(files.clone())?,
            None => Vec::new(),
        };
    files.sort_by(|one, two| one.path.cmp(&two.path));
    let prompt = text(value, "prompt")?.trim();
    Ok(
        json!({ "name": value["name"], "description": value["description"], "prompt": prompt, "files": files }),
    )
}

fn same_version(kind: &str, expected: &Value, received: &Value) -> Result<bool> {
    let mut expected_content = version_content(kind, expected)?;
    let mut received_content = version_content(kind, received)?;
    if kind == "skill"
        && expected_content["files"]
            .as_array()
            .is_some_and(|files| !files.is_empty())
    {
        // 技能包的 prompt 由平台根据完整文件生成。
        expected_content
            .as_object_mut()
            .ok_or(Error::State("技能内容格式无效"))?
            .remove("prompt");
        received_content
            .as_object_mut()
            .ok_or(Error::State("技能内容格式无效"))?
            .remove("prompt");
    }
    Ok(expected["version"] == received["version"]
        && expected["notes"] == received["notes"]
        && expected_content == received_content)
}

impl PlatformClient {
    pub(crate) async fn asset_publication_request(
        &self,
        store: &Store,
        request: &LocalRequest,
    ) -> Result<Option<Value>> {
        for kind in ["skill", "expert"] {
            let Some(path) = request
                .path
                .strip_prefix(&format!("/api/v1/studio/{kind}s/"))
            else {
                continue;
            };
            let Some((id, action)) = path.split_once('/') else {
                continue;
            };
            if request.method == "GET" && action == "publications" {
                return Ok(Some(self.asset_publication_history(store, kind, id).await?));
            }
            if request.method == "POST" && matches!(action, "publish" | "unpublish") {
                return Ok(Some(
                    self.change_asset_publication(
                        store,
                        kind,
                        id,
                        action,
                        request.body.clone().ok_or(Error::Invalid("缺少发布内容"))?,
                    )
                    .await?,
                ));
            }
        }
        Ok(None)
    }

    pub async fn asset_publication_history(
        &self,
        store: &Store,
        kind: &str,
        id: &str,
    ) -> Result<Value> {
        let account = self.account_scope().await?;
        let _update = store.publication_updates.lock().await;
        store.publication_asset(&account, kind, id).await?;
        let Some(mut sync) = store.asset_sync(&account, kind, id).await? else {
            return Ok(json!([]));
        };
        if let Some(remote) = sync["remote_id"].as_str().map(str::to_owned) {
            sync["remote"] = self
                .request(
                    Method::GET,
                    &format!("/api/v1/studio/{kind}s/{remote}"),
                    None,
                )
                .await?;
            sync["publications"] = self
                .request(
                    Method::GET,
                    &format!("/api/v1/studio/{kind}s/{remote}/publications"),
                    None,
                )
                .await?;
            if !sync["publications"].is_array() {
                return Err(Error::State("平台发布记录格式无效"));
            }
            store.save_asset_sync(&account, kind, id, &sync).await?;
        }
        Ok(sync["publications"].clone())
    }

    pub async fn change_asset_publication(
        &self,
        store: &Store,
        kind: &str,
        id: &str,
        action: &str,
        input: Value,
    ) -> Result<Value> {
        if !matches!(kind, "skill" | "expert") || !matches!(action, "publish" | "unpublish") {
            return Err(Error::Invalid("发布操作无效"));
        }
        Uuid::parse_str(id).map_err(|_| Error::Invalid("发布对象标识无效"))?;
        let input: PublicationInput =
            serde_json::from_value(input).map_err(|_| Error::Invalid("发布请求字段无效"))?;
        if input.request_id.trim().is_empty() || input.request_id.len() > 128 {
            return Err(Error::Invalid("发布请求编号无效"));
        }
        if action == "publish" || !input.version.is_empty() {
            crate::skills::stable_version(&input.version)?;
        }
        let account = self.account_scope().await?;
        let _update = store.publication_updates.lock().await;
        let asset = store.publication_asset(&account, kind, id).await?;
        let binding = json!({ "kind": kind, "asset_id": id, "action": action, "version": input.version, "reason": input.reason });
        let previous: Option<String> = sqlx::query_scalar(
            "SELECT document FROM asset_publication_requests WHERE account_id=? AND request_id=?",
        )
        .bind(&account)
        .bind(&input.request_id)
        .fetch_optional(&store.pool)
        .await?;
        let mut operation = match previous {
            Some(document) => {
                let document: Value = serde_json::from_str(&document)?;
                if document["binding"] != binding {
                    return Err(Error::Invalid("发布请求编号已经用于其他内容"));
                }
                if !document["response"].is_null() {
                    return Ok(document["response"].clone());
                }
                document
            }
            None => {
                let document = json!({ "binding": binding, "remote_request_id": Uuid::new_v4().to_string(), "response": null });
                sqlx::query("INSERT INTO asset_publication_requests(account_id,request_id,document) VALUES(?,?,?)")
                    .bind(&account).bind(&input.request_id).bind(document.to_string()).execute(&store.pool).await?;
                document
            }
        };
        if action == "publish" && asset["enabled"] != true {
            return Err(Error::Invalid("启用后才能发布"));
        }
        let mut sync = store.asset_sync(&account, kind, id).await?.unwrap_or_else(|| json!({
            "token": Uuid::new_v4().to_string(), "creation_state": "ready", "remote_id": null,
            "versions": {}, "remote": null, "publications": [], "avatar_requests": {},
        }));
        if action == "publish" {
            store.save_asset_sync(&account, kind, id, &sync).await?;
            let versions = match kind {
                "skill" => store.skill_versions(&account, id).await?,
                _ => store.expert_versions(&account, id).await?,
            };
            let selected = crate::skills::stable_version(&input.version)?;
            let mut selected_versions = Vec::new();
            let mut found = false;
            for version in versions
                .as_array()
                .ok_or(Error::State("本地版本列表格式无效"))?
            {
                let number = crate::skills::stable_version(text(version, "version")?)?;
                if number == selected {
                    found = true;
                }
                if number <= selected {
                    selected_versions.push((number, version.clone()));
                }
            }
            if !found {
                return Err(Error::NotFound);
            }
            selected_versions.sort_by(|one, two| one.0.cmp(&two.0));
            for (_, version) in selected_versions {
                let number = text(&version, "version")?.to_owned();
                if sync["versions"].get(&number).is_none() {
                    let mut payload = if kind == "expert" {
                        self.expert_publication_content(store, &account, id, &version, &mut sync)
                            .await?
                    } else {
                        version_content(kind, &version)?
                    };
                    payload["version"] = json!(number);
                    payload["enabled"] = json!(true);
                    payload["notes"] = json!(format!(
                        "{}\nPixoma Studio 同步编号：{}",
                        version["notes"].as_str().unwrap_or(""),
                        text(&sync, "token")?
                    ));
                    sync["versions"][&number] = payload;
                    store.save_asset_sync(&account, kind, id, &sync).await?;
                }
                self.sync_publication_version(store, &account, kind, id, &number, &mut sync)
                    .await?;
            }
        }
        let remote = sync["remote_id"]
            .as_str()
            .ok_or(Error::Invalid("该对象尚未发布到平台"))?;
        let response = self.request(Method::POST, &format!("/api/v1/studio/{kind}s/{remote}/{action}"), Some(json!({
            "request_id": operation["remote_request_id"], "version": input.version, "reason": input.reason,
        }))).await?;
        if response["id"] != remote {
            return Err(Error::State("平台发布响应标识不一致"));
        }
        sync["publications"] = self
            .request(
                Method::GET,
                &format!("/api/v1/studio/{kind}s/{remote}/publications"),
                None,
            )
            .await?;
        if !sync["publications"].is_array() {
            return Err(Error::State("平台发布记录格式无效"));
        }
        sync["remote"] = response;
        let mut result = asset;
        for field in [
            "published",
            "published_version",
            "published_at",
            "market_locked",
            "market_lock_reason",
        ] {
            if let Some(value) = sync["remote"].get(field) {
                result[field] = value.clone();
            }
        }
        operation["response"] = result.clone();
        let mut transaction = store.pool.begin().await?;
        sqlx::query(
            "UPDATE asset_syncs SET document=? WHERE account_id=? AND kind=? AND asset_id=?",
        )
        .bind(sync.to_string())
        .bind(&account)
        .bind(kind)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE asset_publication_requests SET document=? WHERE account_id=? AND request_id=?",
        )
        .bind(operation.to_string())
        .bind(&account)
        .bind(&input.request_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result)
    }

    async fn sync_publication_version(
        &self,
        store: &Store,
        account: &str,
        kind: &str,
        id: &str,
        number: &str,
        sync: &mut Value,
    ) -> Result<()> {
        let payload = sync["versions"][number].clone();
        if sync["remote_id"].is_null() {
            if sync["creation_state"] == "creating" {
                let initial = sync["versions"][text(sync, "initial_version")?].clone();
                let objects = self
                    .request(Method::GET, &format!("/api/v1/studio/{kind}s"), None)
                    .await?;
                let mut matching = Vec::new();
                for object in objects
                    .as_array()
                    .ok_or(Error::State("平台对象列表格式无效"))?
                {
                    let remote = text(object, "id")?;
                    let path = format!(
                        "/api/v1/studio/{kind}s/{remote}/versions/{}",
                        text(&initial, "version")?
                    );
                    match self.request(Method::GET, &path, None).await {
                        Ok(version) if same_version(kind, &initial, &version)? => {
                            matching.push(remote.to_owned())
                        }
                        Ok(_) | Err(Error::Remote { status: 404, .. }) => {}
                        Err(error) => return Err(error),
                    }
                }
                if matching.len() != 1 {
                    return Err(Error::State(
                        "平台创建结果尚未确认，无法继续发布，请稍后重试",
                    ));
                }
                sync["remote_id"] = json!(matching.remove(0));
            } else {
                sync["initial_version"] = json!(number);
                sync["creation_state"] = json!("creating");
                store.save_asset_sync(account, kind, id, sync).await?;
                let created = self
                    .request(
                        Method::POST,
                        &format!("/api/v1/studio/{kind}s"),
                        Some(payload.clone()),
                    )
                    .await;
                let created = match created {
                    Ok(created) => created,
                    Err(
                        error @ Error::Remote {
                            status: 400..=499, ..
                        },
                    ) => {
                        sync["creation_state"] = json!("ready");
                        store.save_asset_sync(account, kind, id, sync).await?;
                        return Err(error);
                    }
                    Err(error) => return Err(error),
                };
                let remote = text(&created, "id")?;
                Uuid::parse_str(remote).map_err(|_| Error::State("平台对象标识无效"))?;
                sync["remote_id"] = json!(remote);
            }
            sync["creation_state"] = json!("linked");
            store.save_asset_sync(account, kind, id, sync).await?;
        }
        let remote = text(sync, "remote_id")?;
        let path = format!("/api/v1/studio/{kind}s/{remote}");
        let versions = self
            .request(Method::GET, &format!("{path}/versions"), None)
            .await?;
        if versions
            .as_array()
            .ok_or(Error::State("平台版本列表格式无效"))?
            .iter()
            .any(|version| version["version"] == number)
        {
            let existing = self
                .request(Method::GET, &format!("{path}/versions/{number}"), None)
                .await?;
            if !same_version(kind, &payload, &existing)? {
                return Err(Error::State("平台同一版本的内容与本地同步记录不一致"));
            }
            return Ok(());
        }
        let current = self.request(Method::GET, &path, None).await?;
        if crate::skills::stable_version(text(&current, "version")?)?
            >= crate::skills::stable_version(number)?
        {
            return Err(Error::State("平台已经保存更高版本，无法添加所选历史版本"));
        }
        let mut payload = payload;
        payload["updated_at"] = current["updated_at"].clone();
        self.request(Method::PATCH, &path, Some(payload)).await?;
        Ok(())
    }

    async fn expert_publication_content(
        &self,
        store: &Store,
        account: &str,
        id: &str,
        version: &Value,
        sync: &mut Value,
    ) -> Result<Value> {
        let mut content: ExpertContent = serde_json::from_value(version.clone())?;
        content.validate()?;
        let model: String =
            sqlx::query_scalar("SELECT document FROM model_configs WHERE account_id=? AND id=?")
                .bind(account)
                .bind(&content.model_id)
                .fetch_optional(&store.pool)
                .await?
                .ok_or(Error::NotFound)?;
        let model: Value = serde_json::from_str(&model)?;
        let models = self
            .request(Method::GET, "/api/v1/studio/models", None)
            .await?;
        let protocol = text(&model, "protocol")?;
        let address = crate::models::provider_base_url(protocol, text(&model, "base_url")?)?;
        let mut matching = Vec::new();
        for candidate in models
            .as_array()
            .ok_or(Error::State("平台模型列表格式无效"))?
        {
            if candidate["protocol"] == protocol
                && candidate["model"] == model["model"]
                && candidate["enabled"] == true
                && candidate["agent_enabled"] == true
                && candidate["capabilities"]["tools"] == true
                && crate::models::provider_base_url(protocol, text(candidate, "base_url")?)?
                    == address
            {
                matching.push(candidate);
            }
        }
        let selected = if matching.len() == 1 {
            matching[0]
        } else {
            let defaults: Vec<_> = matching
                .into_iter()
                .filter(|model| model["default"] == true)
                .collect();
            if defaults.len() != 1 {
                return Err(Error::Invalid("平台没有唯一匹配的可用模型，无法发布该专家"));
            }
            defaults[0]
        };
        content.model_id = text(selected, "id")?.to_owned();
        for part in content.prompt_parts.iter_mut().chain(
            content
                .sop
                .iter_mut()
                .flat_map(|step| &mut step.instruction_parts),
        ) {
            if let ExpertPart::Skill {
                skill_id, version, ..
            } = part
            {
                let skill = store.get_skill(account, skill_id).await?;
                if skill["source"] == "user" {
                    let linked = store
                        .asset_sync(account, "skill", skill_id)
                        .await?
                        .ok_or(Error::Invalid("先发布专家引用的个人技能"))?;
                    let remote = linked["remote_id"]
                        .as_str()
                        .ok_or(Error::Invalid("先发布专家引用的个人技能"))?;
                    let current = self
                        .request(
                            Method::GET,
                            &format!("/api/v1/studio/skills/{remote}"),
                            None,
                        )
                        .await?;
                    let history = self
                        .request(
                            Method::GET,
                            &format!("/api/v1/studio/skills/{remote}/publications"),
                            None,
                        )
                        .await?;
                    if current["published"] != true
                        || !history
                            .as_array()
                            .ok_or(Error::State("技能发布记录格式无效"))?
                            .iter()
                            .any(|record| {
                                record["action"] == "publish" && record["version"] == *version
                            })
                    {
                        return Err(Error::Invalid("先发布专家引用的个人技能版本"));
                    }
                    *skill_id = remote.to_owned();
                }
            }
        }
        if !content.avatar_asset_id.is_empty() {
            let number = text(version, "version")?;
            if sync["avatar_requests"].get(number).is_none() {
                sync["avatar_requests"][number] = json!(Uuid::new_v4().to_string());
                store.save_asset_sync(account, "expert", id, sync).await?;
            }
            let media = store
                .asset_response(
                    account,
                    &content.avatar_asset_id,
                    &content.avatar_version_id,
                    "GET",
                    None,
                )
                .await?;
            if !media.mime_type.starts_with("image/") || media.body.len() > 5 << 20 {
                return Err(Error::Invalid("专家头像需要使用 5 MiB 以内的图片"));
            }
            let form = reqwest::multipart::Form::new()
                .text(
                    "request_id",
                    text(&sync["avatar_requests"], number)?.to_owned(),
                )
                .part(
                    "file",
                    reqwest::multipart::Part::bytes(media.body)
                        .file_name("avatar")
                        .mime_str(&media.mime_type)?,
                );
            let response = self
                .http
                .post(
                    self.base
                        .join("/api/v1/studio/expert-avatars/upload")
                        .map_err(|_| Error::Invalid("头像上传地址无效"))?,
                )
                .multipart(form)
                .send()
                .await?;
            let avatar = self.response_data(response).await?;
            content.avatar_asset_id = text(&avatar, "id")?.to_owned();
            content.avatar_version_id = text(&avatar["versions"][0], "id")?.to_owned();
        }
        content.validate()?;
        version_content("expert", &serde_json::to_value(content)?)
    }
}
