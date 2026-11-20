use crate::{
    Error, Result, Store,
    assets::digest,
    error::timestamp,
    local_api::{optional_text, text},
    skills::stable_version,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ExpertPart {
    Text {
        text: String,
    },
    Skill {
        skill_id: String,
        version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Workflow {
        workflow_id: String,
        version_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct ExpertStep {
    pub id: String,
    pub title: String,
    pub instruction_parts: Vec<ExpertPart>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct ExpertSkill {
    pub skill_id: String,
    pub version: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct ExpertWorkflow {
    pub workflow_id: String,
    pub version_id: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct ExpertContent {
    pub schema_version: u8,
    pub name: String,
    pub description: String,
    pub model_id: String,
    pub prompt_parts: Vec<ExpertPart>,
    pub avatar_asset_id: String,
    pub avatar_version_id: String,
    pub examples: Vec<String>,
    pub skill_refs: Vec<ExpertSkill>,
    pub workflow_refs: Vec<ExpertWorkflow>,
    pub sop: Vec<ExpertStep>,
}

impl ExpertContent {
    pub(crate) fn validate(&mut self) -> Result<()> {
        self.name = self.name.trim().to_owned();
        if self.schema_version != 2
            || self.name.is_empty()
            || self.name.chars().count() > 256
            || self.model_id.is_empty()
            || self.prompt_parts.is_empty()
            || self.avatar_asset_id.is_empty() != self.avatar_version_id.is_empty()
        {
            return Err(Error::Invalid("专家内容字段无效"));
        }
        let mut steps = HashSet::new();
        for step in &self.sop {
            if step.id.is_empty()
                || step.title.trim().is_empty()
                || step.instruction_parts.is_empty()
                || !steps.insert(&step.id)
            {
                return Err(Error::Invalid("专家步骤缺少内容或标识重复"));
            }
        }
        let mut skills = BTreeMap::new();
        let mut workflows = BTreeMap::new();
        for part in self
            .prompt_parts
            .iter()
            .chain(self.sop.iter().flat_map(|step| &step.instruction_parts))
        {
            match part {
                ExpertPart::Text { text } if text.trim().is_empty() => {
                    return Err(Error::Invalid("专家执行说明不能为空"));
                }
                ExpertPart::Text { .. } => {}
                ExpertPart::Skill {
                    skill_id, version, ..
                } => {
                    if skill_id.is_empty() {
                        return Err(Error::Invalid("专家技能标识不能为空"));
                    }
                    stable_version(version)?;
                    if skills
                        .insert(skill_id.clone(), version.clone())
                        .is_some_and(|previous| previous != *version)
                    {
                        return Err(Error::Invalid("同一技能需要引用相同版本"));
                    }
                }
                ExpertPart::Workflow {
                    workflow_id,
                    version_id,
                    ..
                } => {
                    if workflow_id.is_empty() || version_id.is_empty() {
                        return Err(Error::Invalid("专家工作流引用无效"));
                    }
                    if workflows
                        .insert(workflow_id.clone(), version_id.clone())
                        .is_some_and(|previous| previous != *version_id)
                    {
                        return Err(Error::Invalid("同一工作流需要引用相同版本"));
                    }
                }
            }
        }
        self.skill_refs = skills
            .into_iter()
            .map(|(skill_id, version)| ExpertSkill { skill_id, version })
            .collect();
        self.workflow_refs = workflows
            .into_iter()
            .map(|(workflow_id, version_id)| ExpertWorkflow {
                workflow_id,
                version_id,
            })
            .collect();
        Ok(())
    }
}

impl Store {
    pub(crate) fn expert_model_id(expert: &Value) -> Result<&str> {
        text(expert, "model_id")
    }

    pub(crate) fn expert_context_input(input: &Value, expert: Option<&Value>) -> Result<Value> {
        let Some(expert) = expert else {
            return Ok(input.clone());
        };
        let content: ExpertContent = serde_json::from_value(expert.clone())?;
        let allowed: BTreeMap<&str, &str> = content
            .skill_refs
            .iter()
            .map(|reference| (reference.skill_id.as_str(), reference.version.as_str()))
            .collect();
        for (id, version) in crate::skills::requested_skills(input)? {
            let expected = allowed
                .get(id.as_str())
                .ok_or(Error::Invalid("专家会话只能使用已绑定的技能"))?;
            if version
                .as_deref()
                .is_some_and(|version| version != *expected)
            {
                return Err(Error::Invalid("所选技能版本与专家内容不一致"));
            }
        }
        let allowed_workflows: BTreeMap<&str, &str> = content
            .workflow_refs
            .iter()
            .map(|reference| (reference.workflow_id.as_str(), reference.version_id.as_str()))
            .collect();
        for (id, version) in crate::workflows::requested_workflows(input)? {
            let expected = allowed_workflows
                .get(id.as_str())
                .ok_or(Error::Invalid("专家会话只能使用已绑定的工作流"))?;
            if version.as_deref().is_some_and(|version| version != *expected) {
                return Err(Error::Invalid("所选工作流版本与专家内容不一致"));
            }
        }
        let mut resolved = if input.is_null() {
            json!({})
        } else {
            input.clone()
        };
        let mut parts = match resolved.get("parts").filter(|parts| !parts.is_null()) {
            Some(parts) => parts
                .as_array()
                .ok_or(Error::Invalid("消息内容块需要数组"))?
                .clone(),
            None => Vec::new(),
        };
        for reference in &content.skill_refs {
            parts.push(json!({ "type": "skill_ref", "skill_id": reference.skill_id, "version": reference.version }));
        }
        for reference in &content.workflow_refs {
            parts.push(json!({ "type": "workflow_ref", "workflow_id": reference.workflow_id, "version_id": reference.version_id }));
        }
        resolved["parts"] = json!(parts);
        Ok(resolved)
    }

    pub(crate) async fn list_experts(&self) -> Result<Value> {
        let rows: Vec<String> = sqlx::query_scalar("SELECT document FROM experts ORDER BY json_extract(document,'$.updated_at') DESC,id")
            .fetch_all(&self.pool).await?;
        let mut experts = Vec::new();
        for row in rows {
            let expert = serde_json::from_str(&row)?;
            experts.push(self.expert_availability(expert).await?);
        }
        Ok(json!(experts))
    }

    pub(crate) async fn get_expert(&self, id: &str) -> Result<Value> {
        let document: String =
            sqlx::query_scalar("SELECT document FROM experts WHERE id=?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Error::NotFound)?;
        let expert = serde_json::from_str(&document)?;
        self.expert_availability(expert).await
    }

    async fn expert_availability(&self, mut expert: Value) -> Result<Value> {
        let reason = self.expert_unavailable_reason(&expert).await?;
        expert["available"] = json!(reason.is_none());
        expert["unavailable_reason"] = json!(reason.unwrap_or_default());
        Ok(expert)
    }

    pub(crate) async fn expert_unavailable_reason(
        &self,
        expert: &Value,
    ) -> Result<Option<&'static str>> {
        if expert["enabled"] != true {
            return Ok(Some("专家已停用"));
        }
        let content: ExpertContent = serde_json::from_value(expert.clone())?;
        let model_id = Self::expert_model_id(expert)?;
        let model: Option<String> = sqlx::query_scalar("SELECT document FROM model_configs WHERE id=? AND json_extract(document,'$.enabled')=1 AND json_extract(document,'$.agent_enabled')=1")
            .bind(model_id).fetch_optional(&self.pool).await?;
        let Some(model) = model else {
            return Ok(Some("本地模型不可用于专家调用"));
        };
        let model = serde_json::from_str::<Value>(&model)?;
        if model["capabilities"]["tools"] != true {
            return Ok(Some("本地模型不支持专家工具调用"));
        }
        for reference in content.skill_refs {
            let skill = match self.get_skill(&reference.skill_id).await {
                Ok(skill) => skill,
                Err(Error::NotFound) => return Ok(Some("引用的技能尚未获取")),
                Err(error) => return Err(error),
            };
            if skill["enabled"] != true || skill["available"] == false {
                return Ok(Some("引用的技能已停用或不可用"));
            }
            match self
                .skill_version(&reference.skill_id, &reference.version)
                .await
            {
                Ok(_) => {}
                Err(Error::NotFound) => return Ok(Some("引用的技能版本尚未获取")),
                Err(error) => return Err(error),
            }
        }
        for reference in content.workflow_refs {
            let workflow = match self.get_workflow(&reference.workflow_id).await {
                Ok(workflow) => workflow,
                Err(Error::NotFound) => return Ok(Some("引用的工作流不存在")),
                Err(error) => return Err(error),
            };
            if workflow["enabled"] != true {
                return Ok(Some("引用的工作流已停用"));
            }
            match self.get_workflow_version(&reference.workflow_id, &reference.version_id).await {
                Ok(_) => {}
                Err(Error::NotFound) => return Ok(Some("引用的工作流版本不存在")),
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }

    pub(crate) async fn expert_version(
        &self,
        id: &str,
        version: &str,
    ) -> Result<Value> {
        self.get_expert(id).await?;
        stable_version(version)?;
        let document: String = sqlx::query_scalar(
            "SELECT document FROM expert_versions WHERE expert_id=? AND version=?",
        )
        .bind(id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::NotFound)?;
        Ok(serde_json::from_str(&document)?)
    }

    pub(crate) async fn expert_versions(&self, id: &str) -> Result<Value> {
        self.get_expert(id).await?;
        let rows: Vec<String> = sqlx::query_scalar("SELECT document FROM expert_versions WHERE expert_id=? ORDER BY json_extract(document,'$.created_at') DESC,version DESC")
            .bind(id).fetch_all(&self.pool).await?;
        Ok(json!(
            rows.iter()
                .map(|row| serde_json::from_str::<Value>(row))
                .collect::<std::result::Result<Vec<_>, _>>()?
        ))
    }

    pub(crate) async fn session_expert_capabilities(
        &self,
        session: &str,
    ) -> Result<Value> {
        let snapshot: Option<String> =
            sqlx::query_scalar("SELECT expert_snapshot FROM sessions WHERE id=?")
                .bind(session)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(Error::NotFound)?;
        let Some(snapshot) = snapshot else {
            return Ok(json!({ "skills": [], "workflows": [] }));
        };
        let expert: ExpertContent = serde_json::from_str(&snapshot)?;
        let mut skills = Vec::new();
        for reference in expert.skill_refs {
            let skill = self
                .skill_version(&reference.skill_id, &reference.version)
                .await?;
            skills.push(json!({
                "id": reference.skill_id, "version": reference.version, "name": skill["name"],
                "description": skill["description"], "enabled": true, "has_content": true,
                "created_at": skill["created_at"], "updated_at": skill["updated_at"],
            }));
        }
        let mut workflows = Vec::new();
        for reference in expert.workflow_refs {
            workflows.push(
                self.workflow_version(&reference.workflow_id, Some(&reference.version_id))
                    .await?,
            );
        }
        Ok(json!({ "skills": skills, "workflows": workflows }))
    }

    pub async fn save_expert(
        &self,
        id: Option<&str>,
        input: Value,
    ) -> Result<Value> {
        if serde_json::to_vec(&input)?.len() > 1 << 20 {
            return Err(Error::Invalid("专家内容超过 1 MiB"));
        }
        let mut content: ExpertContent = serde_json::from_value(input.clone())
            .map_err(|_| Error::Invalid("专家内容格式无效"))?;
        content.validate()?;
        let enabled = input["enabled"]
            .as_bool()
            .ok_or(Error::Invalid("专家启停状态无效"))?;
        let version = text(&input, "version")?;
        let notes = optional_text(&input, "notes", "")?;
        let parsed_version = stable_version(version)?;
        let previous = match id {
            Some(id) => Some(self.get_expert(id).await?),
            None => None,
        };
        if let Some(previous) = &previous
            && (input["updated_at"] != previous["updated_at"]
                || parsed_version <= stable_version(text(previous, "version")?)?
            ) {
                return Err(Error::Invalid("专家已更新或版本号没有增加"));
        }
        let model_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM model_configs WHERE id=? AND json_extract(document,'$.enabled')=1 AND json_extract(document,'$.agent_enabled')=1 AND json_extract(document,'$.capabilities.tools')=1)")
            .bind(&content.model_id).fetch_one(&self.pool).await?;
        if !model_exists {
            return Err(Error::Invalid("请选择可供专家使用的本地模型"));
        }
        for reference in &content.skill_refs {
            self.skill_version(&reference.skill_id, &reference.version)
                .await?;
        }
        for reference in &content.workflow_refs {
            self.workflow_version(&reference.workflow_id, Some(&reference.version_id))
                .await?;
        }
        if !content.avatar_asset_id.is_empty() {
            let asset = self.get_asset(&content.avatar_asset_id).await?;
            if !asset.versions.iter().any(|version| {
                version.id == content.avatar_version_id && version.mime_type.starts_with("image/")
            }) {
                return Err(Error::Invalid("专家头像需要可读取的图片版本"));
            }
        }
        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = timestamp()?;
        let mut document = serde_json::to_value(&content)?;
        let fields = document
            .as_object_mut()
            .ok_or(Error::State("专家内容格式无效"))?;
        fields.extend(json!({ "id": id, "version": version, "enabled": enabled, "source": "user",
            "available": enabled,
            "created_at": previous.as_ref().map(|item| item["created_at"].clone()).unwrap_or_else(|| json!(now)), "updated_at": now,
        }).as_object().ok_or(Error::State("专家记录格式无效"))?.clone());
        let mut snapshot = document.clone();
        snapshot["expert_id"] = json!(id);
        snapshot["created_at"] = json!(now);
        snapshot["notes"] = json!(notes);
        let mut transaction = self.pool.begin().await?;
        if let Some(previous) = &previous {
            let changed = sqlx::query("UPDATE experts SET document=? WHERE id=? AND json_extract(document,'$.updated_at')=?")
                .bind(document.to_string()).bind(&id).bind(text(previous, "updated_at")?).execute(&mut *transaction).await?;
            if changed.rows_affected() != 1 {
                return Err(Error::Invalid("专家已更新，请重新读取"));
            }
        } else {
            sqlx::query("INSERT INTO experts(id,source,document) VALUES(?,'user',?)")
                .bind(&id)
                .bind(document.to_string())
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query("INSERT INTO expert_versions(expert_id,version,document,content_digest) VALUES(?,?,?,?)")
            .bind(&id).bind(version).bind(snapshot.to_string()).bind(digest(&serde_json::to_vec(&content)?))
            .execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_expert(&id).await
    }

    pub(crate) async fn set_expert_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<Value> {
        let mut document = self.get_expert(id).await?;
        let previous_updated_at = text(&document, "updated_at")?.to_owned();
        document["enabled"] = json!(enabled);
        document["available"] = json!(enabled);
        document["updated_at"] = json!(timestamp()?);
        let changed = sqlx::query("UPDATE experts SET document=? WHERE id=? AND json_extract(document,'$.updated_at')=?")
            .bind(document.to_string()).bind(id).bind(previous_updated_at).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::Invalid("专家已更新，请重新读取"));
        }
        self.get_expert(id).await
    }

    pub(crate) async fn resolve_expert(
        &self,
        session: &str,
        input: &Value,
    ) -> Result<Option<Value>> {
        if !session.is_empty() {
            self.get_session(session).await?;
            let saved: Option<String> =
                sqlx::query_scalar("SELECT expert_snapshot FROM sessions WHERE id=?")
                    .bind(session)
                    .fetch_one(&self.pool)
                    .await?;
            if let Some(saved) = saved {
                let snapshot: Value = serde_json::from_str(&saved)?;
                for field in ["id", "version"] {
                    let input_field = if field == "id" {
                        "expert_id"
                    } else {
                        "expert_version"
                    };
                    if input[input_field]
                        .as_str()
                        .is_some_and(|value| !value.is_empty() && snapshot[field] != value)
                    {
                        return Err(Error::Invalid("当前会话不能更换专家或版本"));
                    }
                }
                let current = self.get_expert(text(&snapshot, "id")?).await?;
                if current["enabled"] != true || self.expert_unavailable_reason(&snapshot).await?.is_some() {
                    return Err(Error::Invalid("当前专家已停用或不可用"));
                }
                return Ok(Some(snapshot));
            }
            if input["expert_id"].as_str().is_some_and(|id| !id.is_empty()) {
                return Err(Error::Invalid("当前会话不能更换专家"));
            }
            return Ok(None);
        }
        let Some(id) = input["expert_id"].as_str().filter(|id| !id.is_empty()) else {
            return Ok(None);
        };
        let current = self.get_expert(id).await?;
        if current["enabled"] != true {
            return Err(Error::Invalid("当前专家已停用或不可用"));
        }
        let version = input["expert_version"]
            .as_str()
            .filter(|version| !version.is_empty())
            .unwrap_or(text(&current, "version")?);
        let snapshot = self.expert_version(id, version).await?;
        if self.expert_unavailable_reason(&snapshot).await?.is_some() {
            return Err(Error::Invalid("当前专家已停用或不可用"));
        }
        Ok(Some(snapshot))
    }
}
