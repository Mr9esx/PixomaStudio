use crate::{
    Error, Result, Store,
    assets::digest,
    error::{required, timestamp},
};
use rmcp::{
    ServiceExt,
    model::Tool,
    service::{ClientLifecycleMode, ClientServiceExt},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

pub(crate) type McpService = rmcp::service::RunningService<rmcp::RoleClient, McpClient>;

#[derive(Clone)]
pub(crate) struct McpCallContext {
    store: Store,
    run: String,
    id: String,
    tool: crate::tools::RunTool,
    call: rig::message::ToolCall,
    elicitation_gate: std::sync::Arc<tokio::sync::Mutex<()>>,
}

pub(crate) struct McpClient {
    info: rmcp::model::ClientConfig,
    context: Option<McpCallContext>,
}

impl rmcp::ClientHandler for McpClient {
    fn get_info(&self) -> rmcp::model::ClientConfig {
        self.info.clone()
    }

    async fn create_elicitation(
        &self,
        request: rmcp::model::ElicitRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleClient>,
    ) -> std::result::Result<rmcp::model::ElicitResult, rmcp::ErrorData> {
        let call = self
            .context
            .as_ref()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("当前请求没有补充输入会话", None))?;
        call.answer_elicitation(request, context.ct)
            .await
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))
    }
}

impl McpCallContext {
    pub(crate) fn new(
        store: Store,
        run: &str,
        id: &str,
        tool: &crate::tools::RunTool,
        call: &rig::message::ToolCall,
    ) -> Self {
        Self {
            store,
            run: run.to_owned(),
            id: id.to_owned(),
            tool: tool.clone(),
            call: call.clone(),
            elicitation_gate: Default::default(),
        }
    }

    async fn answer_elicitation(
        &self,
        request: rmcp::model::ElicitRequestParams,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<rmcp::model::ElicitResult> {
        let _input = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(Error::Cancelled),
            input = self.elicitation_gate.lock() => input,
        };
        let mut changes = self.store.run_changes.subscribe();
        self.store
            .save_mcp_inputs(
                &self.run,
                &self.id,
                &self.tool,
                &self.call,
                &crate::mcp_inputs::McpInput::Live(request),
            )
            .await?;
        loop {
            changes.borrow_and_update();
            if cancellation.is_cancelled() {
                self.store
                    .close_live_mcp_input(
                        &self.run,
                        &self.id,
                        &self.tool,
                        &self.call,
                    )
                    .await?;
                return Err(Error::Cancelled);
            }
            let mut saved = self
                .store
                .mcp_execution(&self.run, &self.id, &self.tool, &self.call)
                .await?
                .ok_or(Error::State("MCP 继续记录缺失"))?;
            let run = self.store.get_run(&self.run).await?;
            if run["status"] == "running" {
                if saved["status"] == "input_required" {
                    if !self
                        .store
                        .pause_for_mcp_input(
                            &self.run,
                            &self.id,
                            &self.tool,
                            &self.call,
                        )
                        .await?
                    {
                        continue;
                    }
                } else if saved["status"] == "ready" {
                    let result =
                        serde_json::from_value(saved["input"]["responses"]["elicitation"].clone())?;
                    saved["status"] = json!("continuing");
                    self.store
                        .update_mcp_execution(&self.run, &self.id, &saved)
                        .await?;
                    return Ok(result);
                } else {
                    return Err(Error::State("MCP 补充输入状态无效"));
                }
            } else if run["status"] != "waiting_clarification" {
                return Err(Error::Cancelled);
            }
            tokio::select! {
                _ = cancellation.cancelled() => {}
                changed = changes.changed() => changed.map_err(|_| Error::State("MCP 输入通知已关闭"))?,
            }
        }
    }

    pub(crate) async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
    ) -> Result<rmcp::model::CallToolResponse> {
        let crate::tools::ToolTarget::Mcp { name, .. } = &self.tool.target else {
            return Err(Error::State("MCP 工具配置无效"));
        };
        if request.name.as_ref() != name {
            return Err(Error::Invalid("MCP 请求与工具名称不一致"));
        }
        let service = self
            .store
            .connect_tool_service_with_context(&self.tool, Some(self.clone()))
            .await?;
        let response = {
            let mut changes = self.store.run_changes.subscribe();
            let operation = service.call_tool_once(request);
            tokio::pin!(operation);
            let mut remaining = Duration::from_secs(300);
            let mut previous = tokio::time::Instant::now();
            let mut waiting = false;
            let mut keep_alive = tokio::time::interval(Duration::from_secs(30));
            keep_alive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                changes.borrow_and_update();
                if !waiting {
                    remaining = remaining.saturating_sub(previous.elapsed());
                }
                previous = tokio::time::Instant::now();
                waiting = self.store.get_run(&self.run).await?["status"]
                    == "waiting_clarification";
                tokio::select! {
                    response = &mut operation => break response.map_err(|error| Error::ToolResultUnknown(error.to_string())),
                    _ = tokio::time::sleep(remaining), if !waiting => break Err(Error::ToolResultUnknown("MCP 工具请求超时".to_owned())),
                    _ = keep_alive.tick(), if waiting => {
                        let request = rmcp::model::ClientRequest::PingRequest(rmcp::model::PingRequest::default());
                        match tokio::time::timeout(Duration::from_secs(15), service.send_request(request)).await {
                            Ok(Ok(rmcp::model::ServerResult::EmptyResult(_))) => {}
                            Ok(Ok(_)) => break Err(Error::ToolResultUnknown("MCP 连接检查返回了无法识别的结果".to_owned())),
                            Ok(Err(error)) => break Err(Error::ToolResultUnknown(error.to_string())),
                            Err(_) => break Err(Error::ToolResultUnknown("MCP 连接检查超时".to_owned())),
                        }
                    }
                    changed = changes.changed() => changed.map_err(|_| Error::State("MCP 输入通知已关闭"))?,
                }
            }
        };
        let cancelled = service.cancel().await;
        let _input = self.elicitation_gate.lock().await;
        self.store
            .close_live_mcp_input(&self.run, &self.id, &self.tool, &self.call)
            .await?;
        cancelled.map_err(|error| Error::Mcp(error.to_string()))?;
        response
    }

    pub(crate) async fn get_task(&self, id: &str) -> Result<rmcp::model::GetTaskResult> {
        let service = self
            .store
            .connect_tool_service_with_context(&self.tool, Some(self.clone()))
            .await?;
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            service.get_task(rmcp::model::GetTaskParams::new(id)),
        )
        .await;
        service
            .cancel()
            .await
            .map_err(|error| Error::Mcp(error.to_string()))?;
        response
            .map_err(|_| Error::Mcp(format!("MCP 任务 {id} 查询超时")))?
            .map_err(|error| Error::Mcp(format!("MCP 任务 {id} 查询失败：{error}")))
    }

    pub(crate) async fn update_task(
        &self,
        id: &str,
        responses: rmcp::model::InputResponses,
    ) -> Result<()> {
        let service = self
            .store
            .connect_tool_service_with_context(&self.tool, Some(self.clone()))
            .await?;
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            service.update_task(rmcp::model::UpdateTaskParams::new(id, responses)),
        )
        .await;
        service
            .cancel()
            .await
            .map_err(|error| Error::Mcp(error.to_string()))?;
        response
            .map_err(|_| Error::Mcp(format!("MCP 任务 {id} 输入提交超时")))?
            .map_err(|error| Error::Mcp(format!("MCP 任务 {id} 输入提交失败：{error}")))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConnectorInput {
    name: String,
    url: String,
    #[serde(default, skip_serializing)]
    credential: String,
    enabled: bool,
    policy: String,
}

fn validate_url(address: &str) -> Result<()> {
    let address = Url::parse(address).map_err(|_| Error::Invalid("MCP 地址无效"))?;
    if !["http", "https"].contains(&address.scheme())
        || address.host_str().is_none()
        || !address.username().is_empty()
        || address.password().is_some()
        || address.fragment().is_some()
        || address.query().is_some()
    {
        return Err(Error::Invalid("MCP 地址无效"));
    }
    Ok(())
}

async fn connect_service(
    address: &str,
    credential: &str,
    context: Option<McpCallContext>,
) -> Result<McpService> {
    validate_url(address)?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .build()?;
    let mut config = StreamableHttpClientTransportConfig::with_uri(address.to_owned())
        .reinit_on_expired_session(false);
    if !credential.is_empty() {
        config = config.auth_header(credential);
    }
    let transport = || StreamableHttpClientTransport::with_client(http.clone(), config.clone());
    let mut client = rmcp::model::ClientConfig::default();
    client
        .capabilities
        .extensions
        .get_or_insert_with(Default::default)
        .insert(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            Default::default(),
        );
    if context.is_some() {
        client.capabilities.elicitation = Some(
            rmcp::model::ElicitationCapability::new()
                .with_form(
                    rmcp::model::FormElicitationCapability::new().with_schema_validation(true),
                )
                .with_url(rmcp::model::UrlElicitationCapability::new()),
        );
    }
    tokio::time::timeout(Duration::from_secs(30), async {
        let discovered = McpClient {
            info: client.clone(),
            context: context.clone(),
        }
        .serve_with_lifecycle(
            transport(),
            ClientLifecycleMode::Discover {
                preferred_versions: rmcp::model::ProtocolVersion::KNOWN_VERSIONS
                    .iter()
                    .rev()
                    .cloned()
                    .collect(),
            },
        )
        .await;
        client.protocol_version = match discovered {
            Ok(service) => {
                let info = service
                    .peer_info()
                    .ok_or(Error::State("MCP 服务信息缺失"))?;
                if !info.protocol_version.has_initialize() {
                    return Ok(service);
                }
                let version = info.protocol_version.clone();
                service
                    .cancel()
                    .await
                    .map_err(|error| Error::Mcp(error.to_string()))?;
                version
            }
            Err(rmcp::service::ClientInitializeError::JsonRpcError(error))
                if matches!(
                    error.code,
                    rmcp::model::ErrorCode::METHOD_NOT_FOUND
                        | rmcp::model::ErrorCode::INVALID_REQUEST
                ) =>
            {
                rmcp::model::ProtocolVersion::LATEST_WITH_INITIALIZE
            }
            Err(error) => return Err(Error::Mcp(error.to_string())),
        };
        McpClient {
            info: client,
            context,
        }
        .serve(transport())
        .await
        .map_err(|error| Error::Mcp(error.to_string()))
    })
    .await
    .map_err(|_| Error::Mcp("服务初始化超时".to_owned()))?
}

async fn discover_tools(address: &str, credential: &str) -> Result<Vec<Tool>> {
    let service = connect_service(address, credential, None).await?;
    let tools = tokio::time::timeout(Duration::from_secs(30), service.list_all_tools())
        .await
        .map_err(|_| Error::Mcp("读取工具列表超时".to_owned()))?
        .map_err(|error| Error::Mcp(error.to_string()))?;
    service
        .cancel()
        .await
        .map_err(|error| Error::Mcp(error.to_string()))?;
    Ok(tools)
}

fn tool_summaries(tools: &[Tool]) -> Value {
    json!(tools.iter().map(|tool| json!({ "name": tool.name, "description": tool.description.as_deref().unwrap_or("") })).collect::<Vec<_>>())
}

pub(crate) fn model_result_content(output: &Value) -> Result<Vec<rig::message::ToolResultContent>> {
    use rig::message::{ImageMediaType, MimeType, ToolResultContent};
    let mut result: rmcp::model::CallToolResult = serde_json::from_value(output.clone())?;
    let mut content = Vec::new();
    for part in &result.content {
        if let rmcp::model::ContentBlock::Image(image) = part {
            let media_type = ImageMediaType::from_mime_type(&image.mime_type)
                .ok_or(Error::Invalid("MCP 图片格式不受支持"))?;
            content.push(ToolResultContent::image_base64(
                image.data.clone(),
                Some(media_type),
                None,
            ));
        }
    }
    result
        .content
        .retain(|part| !matches!(part, rmcp::model::ContentBlock::Image(_)));
    content.insert(0, ToolResultContent::json(serde_json::to_value(result)?));
    Ok(content)
}

impl Store {
    pub(crate) async fn active_connector_policy(&self, id: &str) -> Result<String> {
        let document: String = sqlx::query_scalar("SELECT document FROM connectors WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        let input: ConnectorInput = serde_json::from_str(&document)?;
        if !input.enabled || input.policy == "forbidden" {
            return Err(Error::Invalid("MCP 连接已停用或禁止调用"));
        }
        Ok(input.policy)
    }

    pub(crate) async fn mcp_run_tools(&self) -> Result<Vec<crate::tools::RunTool>> {
        let connectors = self.list_connectors(true).await?;
        let mut tools = Vec::new();
        for connector in connectors
            .as_array()
            .ok_or(Error::State("MCP 列表格式无效"))?
        {
            let id = crate::local_api::text(connector, "id")?;
            if connector["tools"].is_null() {
                self.probe_connector(id).await?;
            }
            let row = sqlx::query("SELECT document,tools FROM connectors WHERE id=?")
                .bind(id)
                .fetch_one(&self.pool)
                .await?;
            let input: ConnectorInput = serde_json::from_str(row.get("document"))?;
            let available: Vec<Tool> = serde_json::from_str(
                &row.get::<Option<String>, _>("tools")
                    .ok_or(Error::State("MCP 工具列表尚未就绪"))?,
            )?;
            for tool in available {
                let alias = format!(
                    "mcp_{}",
                    &digest(format!("{id}:{}", tool.name).as_bytes())[..32]
                );
                let read_only = tool
                    .annotations
                    .as_ref()
                    .is_some_and(|annotations| annotations.read_only_hint == Some(true));
                tools.push(crate::tools::RunTool {
                    definition: rig::completion::ToolDefinition::new(
                        rig::message::ToolName::new(alias)
                            .map_err(|_| Error::State("MCP 工具名称无效"))?,
                        format!(
                            "{} / {}：{}",
                            input.name,
                            tool.name,
                            tool.description.as_deref().unwrap_or("")
                        ),
                        Value::Object((*tool.input_schema).clone()),
                    ),
                    target: crate::tools::ToolTarget::Mcp {
                        connector_id: id.to_owned(),
                        url: input.url.clone(),
                        name: tool.name.into_owned(),
                    },
                    requires_approval: input.policy == "approval" || !read_only,
                    read_only,
                });
            }
        }
        Ok(tools)
    }

    #[cfg(test)]
    pub(crate) async fn connect_tool_service(
        &self,
        tool: &crate::tools::RunTool,
    ) -> Result<McpService> {
        self.connect_tool_service_with_context(tool, None).await
    }

    async fn connect_tool_service_with_context(
        &self,
        tool: &crate::tools::RunTool,
        context: Option<McpCallContext>,
    ) -> Result<McpService> {
        let crate::tools::ToolTarget::Mcp {
            connector_id, url, ..
        } = &tool.target
        else {
            return Err(Error::State("MCP 工具配置无效"));
        };
        let row = sqlx::query("SELECT document,credential FROM connectors WHERE id=?")
            .bind(connector_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        let current: ConnectorInput = serde_json::from_str(row.get("document"))?;
        if !current.enabled || current.policy == "forbidden" {
            return Err(Error::Invalid("MCP 连接已停用或禁止调用"));
        }
        if current.url != *url {
            return Err(Error::Invalid("MCP 配置已更改，请重新运行"));
        }
        let credential: String = row.get("credential");
        connect_service(url, &credential, context).await
    }

    #[cfg(test)]
    pub(crate) async fn call_mcp_tool(
        &self,
        tool: &crate::tools::RunTool,
        arguments: &serde_json::Map<String, Value>,
    ) -> Result<Value> {
        let crate::tools::ToolTarget::Mcp { name, .. } = &tool.target else {
            return Err(Error::State("MCP 工具配置无效"));
        };
        let response = self
            .call_mcp_tool_once(
                tool,
                rmcp::model::CallToolRequestParams::new(name.clone())
                    .with_arguments(arguments.clone()),
            )
            .await?;
        match response {
            rmcp::model::CallToolResponse::Complete(result) => {
                if result.is_error == Some(true) {
                    return Err(Error::Mcp(serde_json::to_string(&result)?));
                }
                Ok(serde_json::to_value(result)?)
            }
            rmcp::model::CallToolResponse::InputRequired(_) => {
                Err(Error::Invalid("MCP 工具需要补充输入"))
            }
            rmcp::model::CallToolResponse::Task(task) => {
                Err(Error::ToolResultUnknown(serde_json::to_string(&task)?))
            }
            _ => Err(Error::ToolResultUnknown(
                "MCP 服务返回了无法识别的结果".to_owned(),
            )),
        }
    }

    #[cfg(test)]
    pub(crate) async fn call_mcp_tool_once(
        &self,
        tool: &crate::tools::RunTool,
        request: rmcp::model::CallToolRequestParams,
    ) -> Result<rmcp::model::CallToolResponse> {
        let crate::tools::ToolTarget::Mcp { name, .. } = &tool.target else {
            return Err(Error::State("MCP 工具配置无效"));
        };
        if request.name.as_ref() != name {
            return Err(Error::Invalid("MCP 请求与工具名称不一致"));
        }
        let service = self.connect_tool_service(tool).await?;
        let response =
            tokio::time::timeout(Duration::from_secs(300), service.call_tool_once(request)).await;
        service
            .cancel()
            .await
            .map_err(|error| Error::Mcp(error.to_string()))?;
        response
            .map_err(|_| Error::ToolResultUnknown("MCP 工具请求超时".to_owned()))?
            .map_err(|error| Error::ToolResultUnknown(error.to_string()))
    }

    pub(crate) async fn list_connectors(&self, available_only: bool) -> Result<Value> {
        let rows = sqlx::query("SELECT * FROM connectors WHERE (?=0 OR (json_extract(document,'$.enabled')=1 AND json_extract(document,'$.policy')<>'forbidden')) ORDER BY created_at,id")
            .bind(available_only).fetch_all(&self.pool).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let mut document: Value = serde_json::from_str(row.get("document"))?;
            document["id"] = json!(row.get::<String, _>("id"));
            document["credential_masked"] =
                json!(if !row.get::<String, _>("credential").is_empty() {
                    "••••••••"
                } else {
                    ""
                });
            document["tools"] = match row.get::<Option<String>, _>("tools") {
                Some(tools) => tool_summaries(&serde_json::from_str::<Vec<Tool>>(&tools)?),
                None => Value::Null,
            };
            document["created_at"] = json!(row.get::<String, _>("created_at"));
            document["updated_at"] = json!(row.get::<String, _>("updated_at"));
            items.push(document);
        }
        Ok(json!(items))
    }

    pub(crate) async fn save_connector(&self, id: Option<&str>, document: Value) -> Result<Value> {
        let mut input: ConnectorInput =
            serde_json::from_value(document).map_err(|_| Error::Invalid("MCP 配置字段无效"))?;
        input.name = input.name.trim().to_owned();
        input.url = input.url.trim().to_owned();
        required(&input.name, "MCP 名称不能为空")?;
        validate_url(&input.url)?;
        if !["auto", "approval", "forbidden"].contains(&input.policy.as_str()) {
            return Err(Error::Invalid("MCP 调用策略无效"));
        }
        if input.credential.contains(['\r', '\n']) {
            return Err(Error::Invalid("MCP 凭据格式无效"));
        }
        let existing = match id {
            Some(id) => Some(
                sqlx::query("SELECT document,credential,tools FROM connectors WHERE id=?")
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await?
                    .ok_or(Error::NotFound)?,
            ),
            None => None,
        };
        if input.credential.is_empty()
            && let Some(existing) = &existing
        {
            input.credential = existing.get("credential");
        }
        let tools: Option<String> = match existing {
            Some(row)
                if serde_json::from_str::<Value>(row.get("document"))?["url"] == input.url
                    && input.credential == row.get::<String, _>("credential") =>
            {
                row.get("tools")
            }
            _ => None,
        };
        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = timestamp()?;
        sqlx::query("INSERT INTO connectors(id,document,credential,tools,created_at,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET document=excluded.document,credential=excluded.credential,tools=excluded.tools,updated_at=excluded.updated_at")
            .bind(&id).bind(serde_json::to_string(&input)?).bind(&input.credential).bind(tools).bind(&now).bind(&now).execute(&self.pool).await?;
        self.list_connectors(false)
            .await?
            .as_array()
            .ok_or(Error::State("MCP 列表格式无效"))?
            .iter()
            .find(|item| item["id"] == id)
            .cloned()
            .ok_or(Error::NotFound)
    }

    async fn connector_credential(&self, id: &str) -> Result<String> {
        let row = sqlx::query("SELECT credential FROM connectors WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        Ok(row.get("credential"))
    }

    pub(crate) async fn discover_connector(&self, input: Value) -> Result<Value> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Discovery {
            url: String,
            credential: String,
            connector_id: Option<String>,
        }
        let input: Discovery =
            serde_json::from_value(input).map_err(|_| Error::Invalid("MCP 发现参数无效"))?;
        let credential = match input.connector_id {
            Some(id) => {
                let saved = self.connector_credential(&id).await?;
                if input.credential.is_empty() {
                    saved
                } else {
                    input.credential
                }
            }
            None => input.credential,
        };
        Ok(tool_summaries(
            &discover_tools(&input.url, &credential).await?,
        ))
    }

    pub(crate) async fn probe_connector(&self, id: &str) -> Result<Value> {
        let document: String = sqlx::query_scalar("SELECT document FROM connectors WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(Error::NotFound)?;
        let input: ConnectorInput = serde_json::from_str(&document)?;
        let tools = discover_tools(&input.url, &self.connector_credential(id).await?).await?;
        sqlx::query("UPDATE connectors SET tools=?,updated_at=? WHERE id=? AND document=?")
            .bind(serde_json::to_string(&tools)?)
            .bind(timestamp()?)
            .bind(id)
            .bind(document)
            .execute(&self.pool)
            .await?;
        self.list_connectors(false)
            .await?
            .as_array()
            .ok_or(Error::State("MCP 列表格式无效"))?
            .iter()
            .find(|item| item["id"] == id)
            .cloned()
            .ok_or(Error::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connector_settings_read_and_update_the_saved_database_credential()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("connector-settings-")
            .tempdir_in(root)?;
        let database = directory.path().join("studio.sqlite");
        let store = Store::open(&database).await?;
        let mut input = json!({
            "name": "连接设置", "url": "http://127.0.0.1:1/mcp",
            "credential": "initial-token", "enabled": true, "policy": "approval",
        });
        let connector = store.save_connector(None, input.clone()).await?;
        let id = crate::local_api::text(&connector, "id")?;
        assert_eq!(store.connector_credential(id).await?, "initial-token");
        assert!(
            store
                .connector_credential("missing-connector")
                .await
                .is_err()
        );
        input["credential"] = json!("");
        store.save_connector(Some(id), input.clone()).await?;
        assert_eq!(store.connector_credential(id).await?, "initial-token");
        input["credential"] = json!("updated-token");
        let updated = store.save_connector(Some(id), input).await?;
        assert!(!updated.to_string().contains("updated-token"));
        store.close().await;
        let reopened = Store::open(&database).await?;
        assert_eq!(reopened.connector_credential(id).await?, "updated-token");
        reopened.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn cancelled_elicitation_releases_its_question_and_preserves_the_active_gate()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("mcp-input-cancel-")
            .tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        let model = store.save_model(None, json!({
            "name": "输入取消检查", "protocol": "openai_chat_compatible", "base_url": "http://127.0.0.1:18082/v1", "model": "input-state",
            "enabled": true, "agent_enabled": true, "default": true,
            "limits": { "context_window_tokens": 32000, "max_input_tokens": 24000, "max_output_tokens": 4000 },
            "thinking": { "enabled": false }, "capabilities": { "tools": true, "vision": false, "image_output": false, "streaming": true }
        })).await?;
        let session = store.create_session("input-cancel").await?;
        let queued = store
            .queue_run(
                &session.id,
                "input-cancel",
                crate::local_api::text(&model, "id")?,
                "确认输入",
                "full_access",
            )
            .await?;
        let run = crate::local_api::text(&queued["run"], "id")?;
        assert!(store.claim_run(run).await?);
        let tool = crate::tools::RunTool {
            definition: rig::completion::ToolDefinition::new(
                rig::message::ToolName::new("confirm_input")?,
                "确认输入",
                json!({ "type": "object" }),
            ),
            target: crate::tools::ToolTarget::Mcp {
                connector_id: "input-cancel".to_owned(),
                url: "http://127.0.0.1:18082/mcp".to_owned(),
                name: "confirm_input".to_owned(),
            },
            requires_approval: false,
            read_only: true,
        };
        let call = rig::message::ToolCall::from_wire(
            "input-cancel",
            rig::message::ToolFunction::new(tool.definition.name.clone(), json!({})),
        );
        store.save_tool_part(run, 1, 0, &call).await?;
        let record = store.tool_call_records(run, 1).await?.remove(0);
        let id = crate::local_api::text(&record, "id")?;
        sqlx::query("UPDATE tool_calls SET status='executing',definition=? WHERE run_id=? AND id=?")
            .bind(serde_json::to_string(&tool)?).bind(run).bind(id).execute(&store.pool).await?;
        let request: rmcp::model::ElicitRequestParams = serde_json::from_value(json!({
            "mode": "form", "message": "确认输入", "requestedSchema": { "type": "object", "properties": { "confirm": { "type": "boolean" } }, "required": ["confirm"] }
        }))?;
        let context = McpCallContext::new(store.clone(), run, id, &tool, &call);
        let cancellation = tokio_util::sync::CancellationToken::new();
        let worker_context = context.clone();
        let worker_cancellation = cancellation.clone();
        let worker_request = request.clone();
        let worker = tokio::spawn(async move {
            worker_context
                .answer_elicitation(worker_request, worker_cancellation)
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            while store.get_run(run).await?["status"] != "waiting_clarification" {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok::<_, Error>(())
        })
        .await??;
        let before = store.run_events_after(run, 0).await?;
        let cancelled = tokio_util::sync::CancellationToken::new();
        cancelled.cancel();
        assert!(matches!(
            context.answer_elicitation(request, cancelled).await,
            Err(Error::Cancelled)
        ));
        assert_eq!(
            store.get_run(run).await?["status"],
            "waiting_clarification"
        );
        assert_eq!(store.run_events_after(run, 0).await?, before);
        cancellation.cancel();
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(5), worker).await??,
            Err(Error::Cancelled)
        ));
        assert_eq!(store.get_run(run).await?["status"], "running");
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM clarifications WHERE run_id=? AND json_extract(document,'$.status')='pending')")
            .bind(run).fetch_one(&store.pool).await?;
        assert!(!pending);
        assert_eq!(
            store
                .run_events_after(run, 0)
                .await?
                .last()
                .ok_or("继续事件缺失")?["type"],
            "RUN_STARTED"
        );
        store.finish_run(run, "cancelled", None).await?;
        store.close().await;
        Ok(())
    }

    #[test]
    fn image_results_keep_actual_pixels_outside_json_text() -> Result<()> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use rig::message::{DocumentSourceKind, ToolResultContent};
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(32, 32)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|_| Error::State("图片写入失败"))?;
        let encoded = STANDARD.encode(bytes.get_ref());
        let response = rmcp::model::CallToolResult::success(vec![
            rmcp::model::ContentBlock::text("读取图片完成"),
            rmcp::model::ContentBlock::image(encoded.clone(), "image/png"),
        ]);
        let output = serde_json::to_value(response)?;
        let content = model_result_content(&output)?;
        assert!(
            content
                .iter()
                .any(|part| matches!(part, ToolResultContent::Image(image)
            if matches!(&image.data, DocumentSourceKind::Base64(data) if data == &encoded)))
        );
        for part in content {
            if let ToolResultContent::Json { value } = part {
                assert!(!value.to_string().contains(&encoded));
                assert!(value.to_string().contains("读取图片完成"));
            }
        }
        assert!(output.to_string().contains(&encoded));
        Ok(())
    }

    #[tokio::test]
    #[ignore = "需要 PIXOMA_TEST_MCP_URL 与 PIXOMA_TEST_MCP_TOKEN 并连接真实 MCP 服务"]
    async fn real_connector_returns_service_errors_without_losing_the_saved_configuration()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("mcp-errors-")
            .tempdir_in(root)?;
        let store = Store::open(&directory.path().join("studio.sqlite")).await?;
        store.save_connector(None, json!({
            "name": "真实 MCP 错误检查", "url": std::env::var("PIXOMA_TEST_MCP_URL")?,
            "credential": std::env::var("PIXOMA_TEST_MCP_TOKEN")?, "enabled": true, "policy": "auto",
        })).await?;
        let tools = store.mcp_run_tools().await?;
        let get_task = tools
            .iter()
            .find(|tool| tool.display_name() == "get_task")
            .ok_or("任务查询工具缺失")?;
        let arguments = json!({ "task_id": Uuid::new_v4().to_string() });
        let error = store
            .call_mcp_tool(get_task, arguments.as_object().ok_or("任务查询参数无效")?)
            .await
            .expect_err("不存在的任务不能返回成功结果");
        assert!(matches!(error, Error::Mcp(_)), "{error}");
        let list = tools
            .iter()
            .find(|tool| tool.display_name() == "list_workflows")
            .ok_or("工作流列表工具缺失")?;
        let response = store.call_mcp_tool(list, &serde_json::Map::new()).await?;
        assert_ne!(response["isError"], true);
        assert!(
            !store
                .list_connectors(false)
                .await?
                .as_array()
                .ok_or("连接器列表缺失")?
                .is_empty()
        );
        store.close().await;
        Ok(())
    }
}
