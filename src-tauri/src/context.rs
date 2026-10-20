use crate::{Error, Result, Store, local_api::text, settings::ModelInput};
use base64::{Engine, engine::general_purpose::STANDARD};
use rig::{
    completion::Document,
    message::{ImageMediaType, Message, UserContent},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
struct AssetReference {
    #[serde(alias = "assetId")]
    asset_id: String,
    #[serde(alias = "assetVersionId")]
    asset_version_id: String,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct SelectedAsset {
    pub asset_id: String,
    pub asset_version_id: String,
    pub name: String,
    pub mime_type: String,
    pub version: i64,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct RunContext {
    pub assets: Vec<SelectedAsset>,
    pub documents: Vec<Document>,
    pub skills: Vec<Value>,
    pub workflows: Vec<Value>,
    pub expert: Option<Value>,
}

impl Store {
    pub(crate) async fn prepare_run_context(
        &self,
        account: &str,
        input: &Value,
        model: &ModelInput,
        prompt: &str,
        expert: Option<&Value>,
    ) -> Result<(RunContext, Message)> {
        let mut references = Vec::new();
        if let Some(parts) = input.get("parts").filter(|parts| !parts.is_null()) {
            for part in parts
                .as_array()
                .ok_or(Error::Invalid("消息内容块需要数组"))?
            {
                match text(part, "type")? {
                    "asset_ref" => references.push(
                        serde_json::from_value::<AssetReference>(part.clone())
                            .map_err(|_| Error::Invalid("资产引用字段无效"))?,
                    ),
                    "text" | "skill_ref" | "workflow_ref" => {}
                    _ => return Err(Error::Invalid("消息内容块类型无效")),
                }
            }
        }
        if let Some(assets) = input.get("assets").filter(|assets| !assets.is_null()) {
            for asset in assets
                .as_array()
                .ok_or(Error::Invalid("资产引用需要数组"))?
            {
                references.push(
                    serde_json::from_value::<AssetReference>(asset.clone())
                        .map_err(|_| Error::Invalid("资产引用字段无效"))?,
                );
            }
        }
        let mut context = RunContext::default();
        if let Some(expert) = expert {
            let content: crate::experts::ExpertContent = serde_json::from_value(expert.clone())?;
            context.expert = Some(expert.clone());
            context.documents.push(Document {
                id: format!(
                    "expert:{}:{}",
                    text(expert, "id")?,
                    text(expert, "version")?
                ),
                text: format!(
                    "按照用户当前要求使用这份专家提示词和完整 SOP：\n{}",
                    serde_json::to_string(&content)?
                ),
                additional_props: HashMap::from([
                    ("name".to_owned(), content.name),
                    ("expert_id".to_owned(), text(expert, "id")?.to_owned()),
                    ("version".to_owned(), text(expert, "version")?.to_owned()),
                ]),
            });
        }
        let mut media = Vec::new();
        let mut selected = HashSet::new();
        for reference in references {
            if !selected.insert((
                reference.asset_id.clone(),
                reference.asset_version_id.clone(),
            )) {
                continue;
            }
            let asset = self.get_asset(account, &reference.asset_id).await?;
            let version = asset
                .versions
                .iter()
                .find(|version| version.id == reference.asset_version_id)
                .ok_or(Error::NotFound)?;
            let mut document = Document {
                id: format!("asset:{}:{}", asset.id, version.id),
                text: String::new(),
                additional_props: HashMap::from([
                    ("name".to_owned(), asset.name.clone()),
                    ("asset_id".to_owned(), asset.id.clone()),
                    ("asset_version_id".to_owned(), version.id.clone()),
                    ("mime_type".to_owned(), version.mime_type.clone()),
                ]),
            };
            if version.mime_type.starts_with("text/") || version.mime_type == "application/json" {
                document.text =
                    String::from_utf8(self.read_asset(account, &asset.id, &version.id).await?)
                        .map_err(|_| Error::Invalid("参考文档需要 UTF-8 文本编码"))?;
            } else if version.mime_type.starts_with("image/") {
                if !model.capabilities.vision {
                    return Err(Error::Invalid("所选模型不支持图片输入"));
                }
                let media_type = match version.mime_type.as_str() {
                    "image/png" => ImageMediaType::PNG,
                    "image/jpeg" => ImageMediaType::JPEG,
                    "image/webp" => ImageMediaType::WEBP,
                    "image/gif" => ImageMediaType::GIF,
                    _ => return Err(Error::Invalid("模型图片格式不受支持")),
                };
                media.push(UserContent::image_base64(
                    STANDARD.encode(self.read_asset(account, &asset.id, &version.id).await?),
                    Some(media_type),
                    None,
                ));
                document.text = "用户选择的图片已附在消息中。".to_owned();
            } else {
                document.text = "用户选择的媒体或文件，可通过工具使用对应资产版本。".to_owned();
            }
            context.assets.push(SelectedAsset {
                asset_id: asset.id,
                asset_version_id: version.id.clone(),
                name: asset.name,
                mime_type: version.mime_type.clone(),
                version: version.version,
            });
            context.documents.push(document);
        }
        for (id, version) in crate::skills::requested_skills(input)? {
            let current = self.get_skill(account, &id).await?;
            if current["enabled"] != true || current["available"] == false {
                return Err(Error::Invalid("所选技能已停用或不可用"));
            }
            let version = version.as_deref().unwrap_or(text(&current, "version")?);
            let snapshot = self.skill_version(account, &id, version).await?;
            let mut instructions = text(&snapshot, "prompt")?.to_owned();
            if let Some(files) = snapshot["files"].as_array() {
                instructions.push_str("\n\n使用 load_skill 读取本运行保存的技能文件。文件目录：\n");
                let entries: Vec<Value> = files.iter().map(|file| json!({ "path": file["path"], "binary": file["binary"], "directory": file["directory"] })).collect();
                instructions.push_str(&serde_json::to_string(&entries)?);
            }
            context.documents.push(Document {
                id: format!("skill:{id}:{version}"),
                text: instructions,
                additional_props: HashMap::from([
                    ("name".to_owned(), text(&snapshot, "name")?.to_owned()),
                    ("skill_id".to_owned(), id),
                    ("version".to_owned(), version.to_owned()),
                    ("source".to_owned(), text(&current, "source")?.to_owned()),
                ]),
            });
            context.skills.push(snapshot);
        }
        for (id, version) in crate::workflows::requested_workflows(input)? {
            let workflow = self.workflow_version(account, &id, version).await?;
            context.documents.push(Document {
                id: format!("workflow:{id}:{}", workflow["version_id"]),
                text: format!("所引用工作流的目录内容与输入要求：\n{}\n远程 run_workflow 使用 case_id 和 inputs 提交，实际计算版本由服务决定。", serde_json::to_string(&workflow)?),
                additional_props: HashMap::from([
                    ("workflow_id".to_owned(), id),
                    ("name".to_owned(), text(&workflow, "name")?.to_owned()),
                    ("version_id".to_owned(), workflow["version_id"].to_string()),
                ]),
            });
            context.workflows.push(workflow);
        }
        let mut content = vec![UserContent::text(prompt)];
        content.extend(
            context
                .documents
                .iter()
                .map(|document| UserContent::text(document.to_string())),
        );
        content.extend(media);
        Ok((context, Message::User { content }))
    }

    pub(crate) async fn reference_run_assets(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &str,
        run: &str,
        session: &str,
        context: &RunContext,
        now: &str,
    ) -> Result<()> {
        use uuid::Uuid;
        if context.documents.is_empty() {
            return Ok(());
        }
        let turn: i64 =
            sqlx::query_scalar("SELECT count(*) FROM runs WHERE account_id=? AND session_id=?")
                .bind(account)
                .bind(session)
                .fetch_one(&mut **transaction)
                .await?;
        for asset in &context.assets {
            sqlx::query("INSERT INTO asset_usages(id,account_id,session_id,asset_id,asset_version_id,session_title_snapshot,usage_kind,run_id,created_at) SELECT ?,account_id,id,?,?,title,'referenced',?,? FROM sessions WHERE account_id=? AND id=? ON CONFLICT(session_id,asset_id,asset_version_id,usage_kind) DO NOTHING")
                .bind(Uuid::new_v4().to_string()).bind(&asset.asset_id).bind(&asset.asset_version_id).bind(run).bind(now)
                .bind(account).bind(session).execute(&mut **transaction).await?;
            let id = Uuid::new_v4().to_string();
            let document = context
                .documents
                .iter()
                .find(|document| {
                    document.id == format!("asset:{}:{}", asset.asset_id, asset.asset_version_id)
                })
                .ok_or(Error::State("资产上下文内容缺失"))?;
            let event = json!({ "id": id, "run_id": run, "turn_number": turn, "step_number": 0, "kind": "CONTEXT_INJECTED", "source": asset.asset_version_id,
                "detail": format!("引用资产 {}，版本 {}", asset.name, asset.version), "delta_tokens_estimated": crate::context_records::estimated_tokens(&document.to_string()),
                "before_tokens_estimated": null, "after_tokens_estimated": null, "created_at": now });
            sqlx::query("INSERT INTO context_events(id,account_id,session_id,run_id,created_at,document) VALUES(?,?,?,?,?,?)")
                .bind(id).bind(account).bind(session).bind(run).bind(now).bind(event.to_string()).execute(&mut **transaction).await?;
            sqlx::query("INSERT INTO project_assets(id,account_id,project_id,asset_id,asset_version_id,display_name,added_at,updated_at) SELECT ?,account_id,project_id,?,?,?,?,? FROM sessions WHERE account_id=? AND id=? ON CONFLICT DO NOTHING")
                .bind(Uuid::new_v4().to_string()).bind(&asset.asset_id).bind(&asset.asset_version_id).bind(&asset.name).bind(now).bind(now)
                .bind(account).bind(session).execute(&mut **transaction).await?;
        }
        for document in &context.documents {
            let (kind, version_key) = if document.additional_props.contains_key("skill_id") {
                ("技能", "version")
            } else if document.additional_props.contains_key("workflow_id") {
                ("工作流", "version_id")
            } else if document.additional_props.contains_key("expert_id") {
                ("专家", "version")
            } else {
                continue;
            };
            let name = document
                .additional_props
                .get("name")
                .ok_or(Error::State("能力上下文名称缺失"))?;
            let version = document
                .additional_props
                .get(version_key)
                .ok_or(Error::State("能力上下文版本缺失"))?;
            let id = Uuid::new_v4().to_string();
            let event = json!({ "id": id, "run_id": run, "turn_number": turn, "step_number": 0,
                "kind": "CONTEXT_INJECTED", "source": document.id, "detail": format!("引用{kind} {name}，版本 {version}"),
                "delta_tokens_estimated": crate::context_records::estimated_tokens(&document.to_string()),
                "before_tokens_estimated": null, "after_tokens_estimated": null, "created_at": now });
            sqlx::query("INSERT INTO context_events(id,account_id,session_id,run_id,created_at,document) VALUES(?,?,?,?,?,?)")
                .bind(id).bind(account).bind(session).bind(run).bind(now).bind(event.to_string()).execute(&mut **transaction).await?;
        }
        Ok(())
    }

    pub(crate) fn context_asset_parts(context: &RunContext, input: &Value) -> Value {
        let mut parts = input
            .get("parts")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for asset in &context.assets {
            if !parts.iter().any(|part| {
                part["type"] == "asset_ref"
                    && part["asset_id"] == asset.asset_id
                    && part["asset_version_id"] == asset.asset_version_id
            }) {
                parts.push(json!({ "type": "asset_ref", "asset_id": asset.asset_id, "asset_version_id": asset.asset_version_id, "name": asset.name }));
            }
        }
        for skill in &context.skills {
            if let Some(part) = parts
                .iter_mut()
                .find(|part| part["type"] == "skill_ref" && part["skill_id"] == skill["id"])
            {
                part["version"] = skill["version"].clone();
                part["name"] = skill["name"].clone();
            } else {
                parts.push(json!({ "type": "skill_ref", "skill_id": skill["id"], "version": skill["version"], "name": skill["name"] }));
            }
        }
        for workflow in &context.workflows {
            let id = text(workflow, "id").expect("工作流标识缺失");
            let version = workflow["version_id"].clone();
            if let Some(part) = parts
                .iter_mut()
                .find(|part| part["type"] == "workflow_ref" && part["workflow_id"] == id)
            {
                part["version_id"] = version;
                part["name"] = workflow["name"].clone();
            } else {
                parts.push(json!({ "type": "workflow_ref", "workflow_id": id, "version_id": version, "name": workflow["name"] }));
            }
        }
        json!(parts)
    }
}
