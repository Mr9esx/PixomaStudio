use crate::{
    Error, Result, Store,
    assets::digest,
    error::{required, timestamp},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, sqlite::SqliteRow};
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputField {
    pub key: String,
    pub r#type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub skip_allowed: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub preview: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OutputField {
    pub key: String,
    pub r#type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub media_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputBinding {
    pub key: String,
    pub node_id: String,
    pub field_path: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OutputBinding {
    pub key: String,
    pub node_id: String,
    #[serde(default)]
    pub index: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ComfyDefinition {
    pub node_id: String,
    pub workflow: BTreeMap<String, Value>,
    pub input_bindings: Vec<InputBinding>,
    pub output_bindings: Vec<OutputBinding>,
    #[serde(default)]
    pub workflow_filename: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GenerationProvider {
    Seedream,
    Minimax,
    Seedance,
    HttpJson,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GenerationOperation {
    Image,
    Video,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OnlineDefinition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http: Option<crate::http_generation::HttpJsonTemplate>,
    pub provider: GenerationProvider,
    pub operation: GenerationOperation,
    pub connection_id: String,
    pub model: String,
    pub capability: String,
    pub defaults: BTreeMap<String, Value>,
    pub input_mapping: BTreeMap<String, String>,
    pub output_mapping: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ExecutionDefinition {
    Comfyui { execution: ComfyDefinition },
    Online { execution: OnlineDefinition },
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct WorkflowDefinition {
    pub inputs: Vec<InputField>,
    pub input_schema: Value,
    pub outputs: Vec<OutputField>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub usage: String,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub examples: Vec<WorkflowExample>,
    #[serde(flatten)]
    pub execution: ExecutionDefinition,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkflowExample {
    pub title: String,
    pub prompt: String,
    #[serde(default)]
    pub preview: String,
}

fn invalid(field: &str, message: &str) -> Error {
    Error::Validation(format!("{field}：{message}"))
}

impl WorkflowDefinition {
    pub(crate) fn parse(document: Value) -> Result<Self> {
        let fields = document
            .as_object()
            .ok_or(Error::Invalid("工作流定义需要对象"))?;
        if fields.keys().any(|key| {
            ![
                "type",
                "execution",
                "inputs",
                "input_schema",
                "outputs",
                "description",
                "usage",
                "preview",
                "examples",
            ]
            .contains(&key.as_str())
        }) {
            return Err(Error::Invalid("工作流定义包含未知字段"));
        }
        if serde_json::to_vec(&document)?.len() > 8 << 20 {
            return Err(Error::Invalid("工作流定义超过 8 MiB"));
        }
        let definition: Self = serde_json::from_value(document)
            .map_err(|error| invalid("draft", &error.to_string()))?;
        definition.validate()?;
        Ok(definition)
    }

    pub(crate) fn kind(&self) -> &'static str {
        match &self.execution {
            ExecutionDefinition::Comfyui { .. } => "comfyui",
            ExecutionDefinition::Online { .. } => "online",
        }
    }

    fn validate(&self) -> Result<()> {
        if self.input_schema["type"] != "object" {
            return Err(invalid("input_schema", "输入 Schema 需要 object 类型"));
        }
        jsonschema::validator_for(&self.input_schema)
            .map_err(|error| invalid("input_schema", &error.to_string()))?;
        let mut inputs = HashSet::new();
        for field in &self.inputs {
            if field.key.trim().is_empty() || !inputs.insert(field.key.as_str()) {
                return Err(invalid(&field.key, "输入名称不能为空或重复"));
            }
            if ![
                "string", "number", "boolean", "enum", "image", "video", "audio", "file",
            ]
            .contains(&field.r#type.as_str())
            {
                return Err(invalid(&field.key, "输入类型无效"));
            }
            if self.input_schema["properties"].get(&field.key).is_none() {
                return Err(invalid(&field.key, "输入 Schema 缺少字段"));
            }
        }
        let mut outputs = HashSet::new();
        for field in &self.outputs {
            if field.key.trim().is_empty() || !outputs.insert(field.key.as_str()) {
                return Err(invalid(&field.key, "输出名称不能为空或重复"));
            }
            if !["image", "video", "audio", "text", "file"].contains(&field.r#type.as_str()) {
                return Err(invalid(&field.key, "输出类型无效"));
            }
        }
        if outputs.is_empty() {
            return Err(Error::Invalid("工作流需要至少一个输出"));
        }
        match &self.execution {
            ExecutionDefinition::Comfyui { execution } => {
                if !execution.node_id.is_empty() && Uuid::parse_str(&execution.node_id).is_err() {
                    return Err(invalid("node_id", "计算节点标识无效"));
                }
                if execution.workflow.is_empty() {
                    return Err(invalid("workflow", "工作流图不能为空"));
                }
                let mut remaining = BTreeMap::<&str, HashSet<&str>>::new();
                for (id, node) in &execution.workflow {
                    let dependencies = remaining.entry(id.as_str()).or_default();
                    if !node["class_type"]
                        .as_str()
                        .is_some_and(|value| !value.trim().is_empty())
                        || !node["inputs"].is_object()
                    {
                        return Err(invalid(id, "节点需要 class_type 和 inputs"));
                    }
                    for (name, value) in node["inputs"]
                        .as_object()
                        .ok_or(Error::Invalid("节点输入需要对象"))?
                    {
                        if let Some(link) = value.as_array().filter(|link| {
                            link.len() == 2 && link[0].is_string() && link[1].is_number()
                        }) {
                            let source = link[0].as_str().ok_or(Error::Invalid("节点引用无效"))?;
                            if !execution.workflow.contains_key(source)
                                || link[1].as_u64().is_none()
                                || source == id
                            {
                                return Err(invalid(
                                    &format!("{id}.{name}"),
                                    "引用的节点或输出位置无效",
                                ));
                            }
                            dependencies.insert(source);
                        }
                    }
                }
                while !remaining.is_empty() {
                    let ready: Vec<_> = remaining
                        .iter()
                        .filter(|(_, dependencies)| dependencies.is_empty())
                        .map(|(id, _)| *id)
                        .collect();
                    if ready.is_empty() {
                        return Err(invalid("workflow", "节点引用存在循环"));
                    }
                    for id in ready {
                        remaining.remove(id);
                        for dependencies in remaining.values_mut() {
                            dependencies.remove(id);
                        }
                    }
                }
                let mut bound = HashSet::new();
                for binding in &execution.input_bindings {
                    if !inputs.contains(binding.key.as_str()) || !bound.insert(binding.key.as_str())
                    {
                        return Err(invalid(&binding.key, "输入绑定无效或重复"));
                    }
                    let node = execution
                        .workflow
                        .get(&binding.node_id)
                        .ok_or_else(|| invalid(&binding.key, "输入节点不存在"))?;
                    if node["inputs"].get(&binding.field_path).is_none() {
                        return Err(invalid(&binding.key, "输入参数位置不存在"));
                    }
                }
                if bound != inputs {
                    return Err(invalid("input_bindings", "每个输入都需要绑定参数位置"));
                }
                let mut bound = HashSet::new();
                for binding in &execution.output_bindings {
                    if !outputs.contains(binding.key.as_str())
                        || !bound.insert(binding.key.as_str())
                        || !execution.workflow.contains_key(&binding.node_id)
                    {
                        return Err(invalid(&binding.key, "输出绑定无效、重复或节点不存在"));
                    }
                }
                if bound != outputs {
                    return Err(invalid("output_bindings", "每个输出都需要绑定输出位置"));
                }
            }
            ExecutionDefinition::Online { execution } => {
                if !matches!(
                    (&execution.provider, &execution.operation),
                    (GenerationProvider::Seedream, GenerationOperation::Image)
                        | (GenerationProvider::Minimax, GenerationOperation::Video)
                        | (GenerationProvider::Seedance, GenerationOperation::Video)
                        | (GenerationProvider::HttpJson, GenerationOperation::Video)
                ) {
                    return Err(invalid("operation", "生成服务不支持此操作"));
                }
                required(&execution.model, "请选择生成模型")?;
                if let Some(http) = &execution.http { http.validate()?; }
                if matches!(execution.provider, GenerationProvider::HttpJson) && execution.http.is_none() {return Err(invalid("http","HTTP 模板定义缺失"));}
                if matches!(execution.provider, GenerationProvider::HttpJson) {
                    let http = execution.http.as_ref().ok_or_else(||invalid("http","HTTP 模板定义缺失"))?;
                    if execution.input_mapping.values().any(|pointer|!pointer.starts_with('/') || http.body.pointer(pointer).is_none()) {return Err(invalid("input_mapping","HTTP 输入需要映射到请求内容中的 JSON pointer"));}
                    if execution.output_mapping.values().any(|pointer|!pointer.starts_with('/')) {return Err(invalid("output_mapping","HTTP 输出映射需要 JSON pointer"));}
                }
                required(&execution.capability, "请选择模型能力")?;
                if !execution.connection_id.is_empty()
                    && Uuid::parse_str(&execution.connection_id).is_err()
                {
                    return Err(invalid("connection_id", "生成服务标识无效"));
                }
                if execution
                    .input_mapping
                    .keys()
                    .any(|key| !inputs.contains(key.as_str()) && !execution.defaults.contains_key(key))
                    || inputs
                        .iter()
                        .any(|key| !execution.input_mapping.contains_key(*key))
                {
                    return Err(invalid("input_mapping", "每个输入都需要映射生成参数"));
                }
                if execution
                    .output_mapping
                    .keys()
                    .any(|key| !outputs.contains(key.as_str()))
                    || outputs
                        .iter()
                        .any(|key| !execution.output_mapping.contains_key(*key))
                {
                    return Err(invalid("output_mapping", "每个输出都需要映射生成结果"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateWorkflow {
    name: String,
    category: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    enabled: bool,
    draft: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateWorkflow {
    expected_revision: i64,
    name: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    enabled: Option<bool>,
    draft: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateVersion {
    expected_revision: i64,
    version: String,
    #[serde(default)]
    notes: String,
}

fn workflow_row(row: SqliteRow) -> Result<Value> {
    Ok(json!({
        "id": row.get::<String, _>("id"), "name": row.get::<String, _>("name"),
        "type": row.get::<String, _>("type"), "category": row.get::<String, _>("category"),
        "tags": serde_json::from_str::<Value>(row.get("tags"))?, "enabled": row.get::<bool, _>("enabled"),
        "draft": serde_json::from_str::<Value>(row.get("draft"))?, "revision": row.get::<i64, _>("revision"),
        "default_version_id": row.get::<Option<String>, _>("default_version_id"),
        "created_at": row.get::<String, _>("created_at"), "updated_at": row.get::<String, _>("updated_at"),
    }))
}

fn version_row(row: SqliteRow) -> Result<Value> {
    Ok(json!({
        "id": row.get::<String, _>("id"), "workflow_id": row.get::<String, _>("workflow_id"),
        "version": row.get::<String, _>("version"), "definition": serde_json::from_str::<Value>(row.get("definition"))?,
        "content_digest": row.get::<String, _>("content_digest"), "notes": row.get::<String, _>("notes"),
        "created_at": row.get::<String, _>("created_at"),
    }))
}

impl Store {
    pub async fn create_workflow(&self, input: Value) -> Result<Value> {
        let input: CreateWorkflow = serde_json::from_value(input)
            .map_err(|error| invalid("workflow", &error.to_string()))?;
        required(&input.name, "工作流名称不能为空")?;
        required(&input.category, "工作流分类不能为空")?;
        let definition = WorkflowDefinition::parse(input.draft)?;
        let id = Uuid::new_v4().to_string();
        let now = timestamp()?;
        sqlx::query("INSERT INTO workflows(id,name,type,category,tags,enabled,draft,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,1,?,?)")
            .bind(&id).bind(input.name.trim()).bind(definition.kind()).bind(input.category.trim()).bind(serde_json::to_string(&input.tags)?).bind(input.enabled)
            .bind(serde_json::to_string(&definition)?).bind(&now).bind(&now).execute(&self.pool).await?;
        self.get_workflow(&id).await
    }

    pub async fn get_workflow(&self, id: &str) -> Result<Value> {
        let row = sqlx::query("SELECT * FROM workflows WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        workflow_row(row)
    }

    pub async fn list_workflows(
        &self,
        kind: Option<&str>,
        category: Option<&str>,
        enabled: Option<bool>,
        query: Option<&str>,
    ) -> Result<Value> {
        if kind.is_some_and(|kind| !["comfyui", "online"].contains(&kind)) {
            return Err(Error::Invalid("工作流类型无效"));
        }
        let rows = sqlx::query("SELECT * FROM workflows WHERE (? IS NULL OR type=?) AND (? IS NULL OR category=?) AND (? IS NULL OR enabled=?) AND (? IS NULL OR instr(name,?)>0) ORDER BY updated_at DESC,id")
            .bind(kind).bind(kind).bind(category).bind(category).bind(enabled).bind(enabled).bind(query).bind(query).fetch_all(&self.pool).await?;
        Ok(json!(
            rows.into_iter()
                .map(workflow_row)
                .collect::<Result<Vec<_>>>()?
        ))
    }

    pub async fn update_workflow(&self, id: &str, patch: Value) -> Result<Value> {
        let patch: UpdateWorkflow = serde_json::from_value(patch)
            .map_err(|error| invalid("workflow", &error.to_string()))?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT * FROM workflows WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(Error::NotFound)?;
        let mut current = workflow_row(row)?;
        if current["revision"] != patch.expected_revision {
            return Err(Error::Conflict("工作流已更新，请重新读取".to_owned()));
        }
        if let Some(name) = patch.name {
            required(&name, "工作流名称不能为空")?;
            current["name"] = json!(name.trim());
        }
        if let Some(category) = patch.category {
            required(&category, "工作流分类不能为空")?;
            current["category"] = json!(category.trim());
        }
        if let Some(tags) = patch.tags {
            current["tags"] = json!(tags);
        }
        if let Some(enabled) = patch.enabled {
            current["enabled"] = json!(enabled);
        }
        if let Some(draft) = patch.draft {
            let definition = WorkflowDefinition::parse(draft)?;
            if json!(definition.kind()) != current["type"] {
                return Err(Error::Invalid("工作流类型不能更改"));
            }
            current["draft"] = serde_json::to_value(definition)?;
        }
        sqlx::query("UPDATE workflows SET name=?,category=?,tags=?,enabled=?,draft=?,revision=revision+1,updated_at=? WHERE id=? AND revision=?")
            .bind(current["name"].as_str().ok_or(Error::State("工作流名称缺失"))?).bind(current["category"].as_str().ok_or(Error::State("工作流分类缺失"))?)
            .bind(current["tags"].to_string()).bind(current["enabled"].as_bool().ok_or(Error::State("工作流启停状态缺失"))?).bind(current["draft"].to_string())
            .bind(timestamp()?).bind(id).bind(patch.expected_revision).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_workflow(id).await
    }

    pub async fn create_workflow_version(&self, id: &str, input: Value) -> Result<Value> {
        let input: CreateVersion = serde_json::from_value(input)
            .map_err(|error| invalid("version", &error.to_string()))?;
        let version = crate::skills::stable_version(&input.version)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT draft,revision FROM workflows WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(Error::NotFound)?;
        if row.get::<i64, _>("revision") != input.expected_revision {
            return Err(Error::Conflict("工作流已更新，请重新读取".to_owned()));
        }
        let versions: Vec<String> =
            sqlx::query_scalar("SELECT version FROM workflow_versions WHERE workflow_id=?")
                .bind(id)
                .fetch_all(&mut *transaction)
                .await?;
        for previous in versions {
            if version <= crate::skills::stable_version(&previous)? {
                return Err(Error::Conflict("版本号需要大于已保存的版本".to_owned()));
            }
        }
        let definition = WorkflowDefinition::parse(serde_json::from_str(row.get("draft"))?)?;
        let mut content = serde_json::to_value(definition)?;
        content.sort_all_objects();
        let content = serde_json::to_string(&content)?;
        let version_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO workflow_versions(id,workflow_id,version,definition,content_digest,notes,created_at) VALUES(?,?,?,?,?,?,?)")
            .bind(&version_id).bind(id).bind(input.version).bind(&content).bind(digest(content.as_bytes())).bind(input.notes).bind(timestamp()?).execute(&mut *transaction).await?;
        transaction.commit().await?;
        self.get_workflow_version(id, &version_id).await
    }

    pub async fn get_workflow_version(&self, id: &str, version: &str) -> Result<Value> {
        let row = sqlx::query("SELECT * FROM workflow_versions WHERE workflow_id=? AND id=?")
            .bind(id)
            .bind(version)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        version_row(row)
    }

    pub async fn list_workflow_versions(&self, id: &str) -> Result<Value> {
        self.get_workflow(id).await?;
        let rows = sqlx::query(
            "SELECT * FROM workflow_versions WHERE workflow_id=? ORDER BY created_at DESC,id",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        Ok(json!(
            rows.into_iter()
                .map(version_row)
                .collect::<Result<Vec<_>>>()?
        ))
    }

    pub async fn set_default_workflow_version(&self, id: &str, version: &str) -> Result<Value> {
        let changed = sqlx::query("UPDATE workflows SET default_version_id=?,revision=revision+1,updated_at=? WHERE id=? AND EXISTS(SELECT 1 FROM workflow_versions WHERE id=? AND workflow_id=?)")
            .bind(version).bind(timestamp()?).bind(id).bind(version).bind(id).execute(&self.pool).await?;
        if changed.rows_affected() != 1 {
            return Err(Error::NotFound);
        }
        self.get_workflow(id).await
    }

    pub async fn delete_workflow(&self, id: &str) -> Result<()> {
        let references: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM expert_versions e,json_each(e.document,'$.workflow_refs') r WHERE json_extract(r.value,'$.workflow_id')=?)")
            .bind(id).fetch_one(&self.pool).await?;
        if references {
            return Err(Error::Conflict(
                "工作流已被专家版本引用，可以停用".to_owned(),
            ));
        }
        let changed = sqlx::query("DELETE FROM workflows WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if changed.rows_affected() != 1 {
            return Err(Error::NotFound);
        }
        Ok(())
    }
}
